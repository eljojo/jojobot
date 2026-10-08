use super::*;
use crate::memory::{Boot, FactId, FactStatus, Provenance, Standing};

/// 🚨 **A filter is a record on the view, its own fields carrying the
/// key, the value, the comparison and the scope** — the same shape
/// every other claim on this store already is, so a view needs no
/// filter syntax of its own and no cap of one filter per key: several
/// filters are several records.
///
/// **Equals needs no `compare` field; `scope` defaults to `thing`.**
/// Both are named explicitly on the second filter here, to prove the
/// non-default path rather than only the common one.
#[test]
fn a_views_own_records_become_its_filters() {
    let held: std::collections::BTreeMap<String, String> =
        [("selects".to_string(), "rhythm".to_string())]
            .into_iter()
            .collect();
    let facts = vec![
        Fact {
            fields: [
                ("key".to_string(), "status".to_string()),
                ("value".to_string(), "active".to_string()),
            ]
            .into_iter()
            .collect(),
            ..fact("view:contract-asked-by-view", "f1", "status is active")
        },
        Fact {
            fields: [
                ("key".to_string(), "donuts_eaten".to_string()),
                ("value".to_string(), "3".to_string()),
                ("compare".to_string(), "greater".to_string()),
                ("scope".to_string(), "record".to_string()),
            ]
            .into_iter()
            .collect(),
            ..fact(
                "view:contract-asked-by-view",
                "f2",
                "more than three donuts",
            )
        },
    ];
    let asked = asked_by_view(&held, &facts);
    assert_eq!(asked.selects.as_deref(), Some("rhythm"));
    assert_eq!(
        asked.filters.len(),
        2,
        "both records must become filters: {:?}",
        asked.filters
    );
    let status = asked
        .filters
        .iter()
        .find(|f| f.key.as_deref() == Some("status"))
        .expect("the status record must be a filter");
    assert_eq!(status.value.as_deref(), Some("active"));
    assert_eq!(status.compare, types::Compare::Equals);
    assert_eq!(status.scope, Scope::Thing);
    let donuts = asked
        .filters
        .iter()
        .find(|f| f.key.as_deref() == Some("donuts_eaten"))
        .expect("the donuts_eaten record must be a filter");
    assert_eq!(donuts.value.as_deref(), Some("3"));
    assert_eq!(donuts.compare, types::Compare::Greater);
    assert_eq!(donuts.scope, Scope::Record);
}

/// **A retracted filter record stops filtering.** The view's records
/// are read the same way any object's records are: an archived one is
/// history, not a live question.
#[test]
fn a_retracted_filter_record_is_not_asked() {
    let held = std::collections::BTreeMap::new();
    let facts = vec![Fact {
        status: FactStatus::Archived,
        fields: [
            ("key".to_string(), "status".to_string()),
            ("value".to_string(), "active".to_string()),
        ]
        .into_iter()
        .collect(),
        ..fact("view:contract-asked-by-view", "f1", "status is active")
    }];
    let asked = asked_by_view(&held, &facts);
    assert!(
        asked.filters.is_empty(),
        "a retracted filter record must not be asked: {:?}",
        asked.filters
    );
}

/// 🚨 **A view can name a type to select structurally, and a relation
/// walk with its own direction, depth and fits_type** — both unresolved,
/// because resolving a name to a declaration needs a read this pure
/// function cannot make.
///
/// **Paired with the absence**: a view naming neither must answer
/// `None` for the walk, not a walk that names nothing — those are
/// different questions, the same distinction `args.follow` already
/// draws for a caller.
#[test]
fn a_views_own_fields_name_a_type_and_a_walk() {
    let held: std::collections::BTreeMap<String, String> = [
        ("selects".to_string(), "person".to_string()),
        ("answers_type".to_string(), "pet-owner".to_string()),
        ("follow_relation".to_string(), "owner".to_string()),
        ("follow_direction".to_string(), "in".to_string()),
        ("follow_depth".to_string(), "2".to_string()),
        ("follow_fits_type".to_string(), "pet".to_string()),
    ]
    .into_iter()
    .collect();
    let asked = asked_by_view(&held, &[]);
    assert_eq!(asked.answers_type.as_deref(), Some("pet-owner"));
    let follow = asked.follow.expect("a walk was named");
    assert_eq!(follow.relation.as_deref(), Some("owner"));
    assert_eq!(follow.shape, None);
    assert_eq!(follow.direction.as_deref(), Some("in"));
    assert_eq!(follow.depth, Some(2));
    assert_eq!(follow.fits_type.as_deref(), Some("pet"));

    let bare: std::collections::BTreeMap<String, String> =
        [("selects".to_string(), "person".to_string())]
            .into_iter()
            .collect();
    let unasked = asked_by_view(&bare, &[]);
    assert_eq!(unasked.answers_type, None);
    assert_eq!(
        unasked.follow, None,
        "a view naming no walk must answer no walk, not one that names nothing",
    );
}

/// **A view names the keys of each object it shows, as a list in one field.**
/// `shows_keys` is read as the comma-separated list `shows` is, trimmed, and
/// a view naming none — or only blanks — answers `None`: every key, which is the
/// ordinary read and a different question from an empty list.
#[test]
fn a_views_own_fields_name_the_keys_to_show() {
    let named: std::collections::BTreeMap<String, String> = [
        ("selects".to_string(), "bot".to_string()),
        (
            "shows_keys".to_string(),
            " one_liner , reports_to ,".to_string(),
        ),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        asked_by_view(&named, &[]).keys,
        Some(vec!["one_liner".to_string(), "reports_to".to_string()]),
    );

    let bare: std::collections::BTreeMap<String, String> =
        [("selects".to_string(), "bot".to_string())]
            .into_iter()
            .collect();
    assert_eq!(asked_by_view(&bare, &[]).keys, None);

    let blank: std::collections::BTreeMap<String, String> = [
        ("selects".to_string(), "bot".to_string()),
        ("shows_keys".to_string(), " , ".to_string()),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        asked_by_view(&blank, &[]).keys,
        None,
        "a list of blanks names no key, so it narrows nothing",
    );
}

fn entity(handle: &str, name: &str) -> Entity {
    // **The fixture stands a store up, because the set is setup here.**
    // Reading a handle asks the kinds this process loaded, and no case
    // behind this fixture asserts anything about the set — so the set
    // arrives the way a boot delivers it, from what a store holds.
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let id = EntityId(handle.to_string());
    Entity {
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

fn fact(home: &str, id: &str, content: &str) -> Fact {
    Fact {
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
    }
}

/// A doc holding one entity, its prose and its rows.
///
/// **The store is what folds a thing's fields, so the fixture stands in for
/// it here.** Every doc below writes each of its keys once, which is the
/// case where the order of the records and the order of the writes agree; a
/// fixture that needs them to disagree says what the thing holds itself,
/// with [`doc_holding`].
fn doc(entity: Entity, prose: &str, facts: Vec<Fact>) -> DocScan {
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
fn doc_holding(
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
    }
}

/// An owned document: readable by one identity and nobody else.
fn owned_by(entity: Entity, owner: &str, prose: &str) -> DocScan {
    DocScan {
        owner: Some(EntityId(owner.to_string())),
        ..doc(entity, prose, Vec::new())
    }
}

/// **An owned object comes back to its owner and to nobody else.**
///
/// Both halves in one read: the caller finds its own, and does not find
/// another's. A third identity is asked the same question, so this is a
/// filter rather than a list of one bot's things.
///
/// ⚠️ **The withheld COUNT is not here yet, and until it is these two
/// answers are the same empty list to a caller** — one that may not read a
/// thing, and one for which there is nothing. Nothing declares an owner
/// yet, so no caller meets that today.
///
/// The unowned object is here because it is the whole rest of the store:
/// nothing that exists today declares an owner, and a filter that hid
/// unowned things would empty every instance.
#[test]
fn an_owned_object_answers_its_owner_and_nobody_else() {
    let scanned = vec![
        owned_by(entity("bot:gamma", "Gamma"), "bot:gamma", ""),
        owned_by(entity("bot:delta", "Delta"), "bot:delta", ""),
        doc(entity("bot:otto", "Otto"), "", Vec::new()),
    ];
    let asking = |who: Option<&str>| GraphQuery {
        select: Selection {
            kind: Some(EntityKind::BOT),
            asked_by: who.map(|w| EntityId(w.to_string())),
            ..Selection::default()
        },
        include: Include {
            facts: false,
            prose: false,
            stood_for: false,
        },
        follow: None,
        history: None,
    };

    let mine = resolved(&scanned, &[], &asking(Some("bot:gamma"))).expect("a kind selects");
    assert_eq!(
        handles(&mine),
        vec!["bot:gamma", "bot:otto"],
        "the caller's own owned object, and everything owned by nobody",
    );

    let theirs = resolved(&scanned, &[], &asking(Some("bot:delta"))).expect("a kind selects");
    assert_eq!(
        handles(&theirs),
        vec!["bot:delta", "bot:otto"],
        "…and another identity sees its own instead, so this is a filter rather than a \
             list of one bot's things",
    );

    let anonymous = resolved(&scanned, &[], &asking(None)).expect("a kind selects");
    assert_eq!(
        handles(&anonymous),
        vec!["bot:otto"],
        "a caller with no identity reaches everything unowned and nothing owned",
    );
}

/// **What was withheld is counted, so a refusal cannot read as an empty
/// store.**
///
/// 🚨 **This is the whole reason the axis exists.** A bot that may not read
/// something and a bot for which there is nothing both get an empty list,
/// and the first reads as the second — so a session searching its own past
/// concludes there is nothing rather than that it was not allowed.
///
/// **A TOTAL and never a breakdown.** How many is work status; which
/// colleague holds them is a directory of who is busy, and the caller named
/// no handle to earn that.
///
/// Both halves: the caller's own are returned AND the rest are counted.
/// Without the count this passes on a build that filters silently.
#[test]
fn what_a_selection_withheld_is_counted_rather_than_dropped_in_silence() {
    let scanned = vec![
        owned_by(entity("bot:gamma", "Gamma"), "bot:gamma", ""),
        owned_by(entity("bot:delta", "Delta"), "bot:delta", ""),
        owned_by(entity("bot:epsilon", "Epsilon"), "bot:epsilon", ""),
        doc(entity("bot:otto", "Otto"), "", Vec::new()),
    ];
    let query = GraphQuery {
        select: Selection {
            kind: Some(EntityKind::BOT),
            asked_by: Some(EntityId("bot:gamma".into())),
            ..Selection::default()
        },
        include: Include {
            facts: false,
            prose: false,
            stood_for: false,
        },
        follow: None,
        history: None,
    };

    let answer = resolve(&scanned, &[], &query).expect("a kind selects");

    assert_eq!(
        handles(&answer.objects),
        vec!["bot:gamma", "bot:otto"],
        "the caller's own, and everything owned by nobody",
    );
    assert_eq!(
        answer.withheld, 2,
        "…and the two belonging to other identities are counted rather than vanishing",
    );
}

/// **Naming another identity's object is refused, and the refusal is not
/// the one a typo gets.**
///
/// ⚠️ **Selecting by kind filters; naming a handle does not.** A named
/// subject returns that object before any filter runs, which is right for
/// every reason it was written — and wrong the moment an object has an
/// owner, because the filter is what keeps it private.
///
/// ⛔️ **It must not answer "no such thing" either.** The caller is holding
/// the handle, so denying the object exists hides nothing and only makes
/// the answer untrustworthy — the defect closed on the write path at
/// `c733f74`, arriving on the read path.
///
/// Both halves: the owner still reads it by name.
#[test]
fn naming_another_identitys_object_is_refused_and_its_owner_still_reads_it() {
    let scanned = vec![owned_by(
        entity("bot:gamma", "Gamma"),
        "bot:gamma",
        "what gamma is for",
    )];
    let naming = |who: &str| GraphQuery {
        select: Selection {
            subject: Some(EntityId("bot:gamma".into())),
            asked_by: Some(EntityId(who.to_string())),
            ..Selection::default()
        },
        include: Include {
            facts: false,
            prose: true,
            stood_for: false,
        },
        follow: None,
        history: None,
    };

    let refused = resolve(&scanned, &[], &naming("bot:delta"));
    match refused {
        Err(MemoryError::NotYours { attempted, .. }) => {
            assert_eq!(attempted, "bot:gamma");
        }
        other => panic!(
            "another identity's object is refused as somebody else's, never as absent \
                 and never returned: {other:?}"
        ),
    }

    let mine = resolved(&scanned, &[], &naming("bot:gamma")).expect("its owner reads it");
    assert_eq!(handles(&mine), vec!["bot:gamma"]);
    assert_eq!(
        mine[0].prose.as_deref(),
        Some("what gamma is for"),
        "…and reads it whole, so this is a refusal of others rather than of everyone",
    );
}

/// A minimal run, projected — the same shape [`crate::session::projected`]'s
/// own test builds, factored out so this file's cases can construct one
/// per bot without repeating the fixture.
fn a_run(bot: &str, id: &str, focus: &str, text: &str) -> DocScan {
    // **The same reason `entity`'s own fixture stands a store up**: the
    // kind set arrives the way a boot delivers it, and a session's own
    // subject is validated against it exactly as any other handle is.
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    use crate::session::{EntryId, JournalEntry, Session, SessionId, SessionState};
    crate::session::projected(&Session {
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

/// 🚨 **The owner-scoping mechanism already covers a run, on the exact
/// shape [`crate::session::projected`] already produces — this is what
/// makes widening the fetch to every bot's runs safe.** Nothing here is
/// new: `resolve`'s owner check reads `DocScan::owner`
/// (`Ctx::of`, `readable_by`, `withheld`), and a run's projection
/// already sets it. This proves the mechanism BEFORE anything fetches a
/// run that is not the caller's own, on purpose — landed first, so an
/// interruption after this commit changes no caller's exposure.
///
/// Three things in one case, because a real caller meets all three at
/// once: naming another bot's run by handle is refused as `NotYours`
/// (never `UnknownEntity` — the caller is holding a real handle);
/// browsing by kind returns only the caller's own and counts the rest as
/// `withheld`, never silently; and the caller's own run still reads
/// whole, prose included, so protecting a run never breaks reading it.
#[test]
fn a_runs_owner_scoping_already_works_on_its_projected_shape() {
    let mine = a_run(
        "bot:gamma",
        "contract-gamma-run",
        "gamma's own",
        "gamma's own beat",
    );
    let theirs = a_run(
        "bot:delta",
        "contract-delta-run",
        "delta's own",
        "delta's own beat",
    );
    let scanned = vec![mine.clone(), theirs.clone()];

    // Naming the other bot's run: refused as theirs, not as absent.
    let naming = GraphQuery {
        select: Selection {
            subject: Some(EntityId("session:contract-delta-run".into())),
            asked_by: Some(EntityId("bot:gamma".into())),
            ..Selection::default()
        },
        include: Include {
            facts: false,
            prose: true,
            stood_for: false,
        },
        follow: None,
        history: None,
    };
    match resolve(&scanned, &[], &naming) {
        Err(MemoryError::NotYours { attempted }) => {
            assert_eq!(attempted, "session:contract-delta-run");
        }
        other => panic!(
            "another bot's run must be refused as somebody else's, never as absent and \
                 never returned: {other:?}"
        ),
    }

    // Browsing by kind: only the caller's own comes back, and the rest
    // is counted rather than vanishing.
    let browsing = GraphQuery {
        select: Selection {
            kind: Some(EntityKind::SESSION),
            asked_by: Some(EntityId("bot:gamma".into())),
            ..Selection::default()
        },
        include: Include {
            facts: false,
            prose: true,
            stood_for: false,
        },
        follow: None,
        history: None,
    };
    let browsed = resolve(&scanned, &[], &browsing).expect("a kind selects");
    assert_eq!(
        handles(&browsed.objects),
        vec!["session:contract-gamma-run"],
        "a bot must find its own past runs and not another's"
    );
    assert_eq!(
        browsed.withheld, 1,
        "the other bot's run is counted as withheld, not dropped in silence — an empty \
             withheld total here would be indistinguishable from an index holding nothing"
    );

    // The caller's own still reads whole — protecting is not the same
    // defect the projection trap would be.
    let mine_query = GraphQuery {
        select: Selection {
            subject: Some(EntityId("session:contract-gamma-run".into())),
            asked_by: Some(EntityId("bot:gamma".into())),
            ..Selection::default()
        },
        include: Include {
            facts: false,
            prose: true,
            stood_for: false,
        },
        follow: None,
        history: None,
    };
    let mine_read = resolved(&scanned, &[], &mine_query).expect("its own bot reads it");
    assert_eq!(handles(&mine_read), vec!["session:contract-gamma-run"]);
    assert_eq!(
        mine_read[0].prose.as_deref(),
        Some("gamma's own beat"),
        "a run selectable to its own bot must also read whole, never with empty prose"
    );
}

/// The objects a selection answered with — what almost every case here
/// asks for. The cases about what was WITHHELD call `resolve` directly,
/// because the count is the thing they assert.
fn resolved(
    scanned: &[DocScan],
    declarations: &[types::DeclaredType],
    query: &GraphQuery,
) -> Result<Vec<Object>, MemoryError> {
    resolve(scanned, declarations, query).map(|answer| answer.objects)
}

/// **What else was recorded around this day.**
///
/// Every claim carries the day it is true of, and nothing could be asked
/// about it associatively. Real questions arrive shaped as *when plus
/// who* — *what did he say back in August* — and the store held the answer
/// with no way to be asked for it.
///
/// **Both halves in one case.** A window that kept everything would pass a
/// check that only looked for the near record, and a dead build that kept
/// nothing would pass one that only looked for the far one missing.
#[test]
fn a_selection_near_a_day_keeps_the_records_in_its_window_and_drops_the_rest() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let dated = |id: &str, day: &str, content: &str| Fact {
        recorded_at: day.parse().expect("a civil date"),
        ..fact("person:milhouse", id, content)
    };
    let scanned = vec![doc(
        entity("person:milhouse", "Milhouse"),
        "His page.",
        vec![
            dated("f1", "2026-08-16", "said the thing about the committee"),
            dated("f2", "2026-02-01", "said something in February"),
        ],
    )];
    let query = GraphQuery {
        select: Selection {
            near: Some(Nearness {
                day: "2026-08-18".parse().expect("a civil date"),
                within_days: 7,
                clock: Clock::RecordedOn,
            }),
            ..Selection::default()
        },
        include: Include {
            facts: true,
            ..GraphQuery::default().include
        },
        ..GraphQuery::default()
    };
    let objects = resolved(&scanned, &[], &query).expect("a read, not an error");
    let content: Vec<&str> = objects
        .iter()
        .flat_map(|o| o.facts.iter())
        .map(|f| f.content.as_str())
        .collect();
    assert!(
        content.iter().any(|c| c.contains("committee")),
        "a record two days from the day asked about is not in the answer: {content:?}",
    );
    assert!(
        !content.iter().any(|c| c.contains("February")),
        "a record six months away came back, so the window keeps everything: {content:?}",
    );

    // 🚨 **A thing planned for next year does not sit in next year.** The
    // booking below is recorded today and carries 2027 as the day it
    // happens. Before the split there was one date field, so recording it
    // put 2027 on the claim — and the claim then answered a window around
    // 2027 and NOT one around the day it was written, which is where a
    // caller asking what was recorded this week looks for it.
    //
    // **Both halves**: it comes back for the day it was recorded on, and
    // it does not come back for the day it happens.
    let booking = Fact {
        happened_at: Some("2027-06-01".parse().expect("a civil date")),
        ..dated("f3", "2026-08-16", "booked the trip for next June")
    };
    let with_booking = vec![doc(
        entity("person:milhouse", "Milhouse"),
        "His page.",
        vec![booking],
    )];
    let recorded_week = resolve(&with_booking, &[], &query).expect("a read");
    assert_eq!(
        recorded_week
            .objects
            .iter()
            .flat_map(|o| o.facts.iter())
            .count(),
        1,
        "a claim recorded two days from the day asked about did not come back",
    );
    let next_year = resolve(
        &with_booking,
        &[],
        &GraphQuery {
            select: Selection {
                near: Some(Nearness {
                    day: "2027-06-01".parse().expect("a civil date"),
                    within_days: 7,
                    clock: Clock::RecordedOn,
                }),
                ..Selection::default()
            },
            ..query.clone()
        },
    )
    .expect("a read");
    assert_eq!(
        next_year
            .objects
            .iter()
            .flat_map(|o| o.facts.iter())
            .count(),
        0,
        "the day the thing HAPPENS placed the claim, so a booking sits in next year",
    );

    // 🚨 **`Clock::HappenedAt` is the clock that DOES place a claim by the
    // day it happens.** Both halves, on the same booking: it comes back
    // for a window around the day it happens, and it does NOT come back
    // for a window around the day it was recorded — the mirror image of
    // `RecordedOn` above, proving this clock reads its own field rather
    // than falling back to another one.
    let asking_happened_at = |day: &str| GraphQuery {
        select: Selection {
            near: Some(Nearness {
                day: day.parse().expect("a civil date"),
                within_days: 7,
                clock: Clock::HappenedAt,
            }),
            ..Selection::default()
        },
        include: Include {
            facts: true,
            ..GraphQuery::default().include
        },
        ..GraphQuery::default()
    };
    let near_the_event =
        resolve(&with_booking, &[], &asking_happened_at("2027-06-04")).expect("a read");
    assert_eq!(
        near_the_event
            .objects
            .iter()
            .flat_map(|o| o.facts.iter())
            .count(),
        1,
        "a window around the day the booking HAPPENS did not find it",
    );
    let near_the_recording =
        resolve(&with_booking, &[], &asking_happened_at("2026-08-18")).expect("a read");
    assert_eq!(
        near_the_recording
            .objects
            .iter()
            .flat_map(|o| o.facts.iter())
            .count(),
        0,
        "a window around the day the booking was RECORDED found it on the happened-at \
             clock, so this clock is reading the wrong field",
    );
}

/// 🚨 **A span is near across its whole length, not only at its start.**
///
/// A fact whose `happened_through` names an end used to be placed by
/// `happened_at` alone, so a day squarely inside the recorded span read as
/// far from it once the window no longer reached back to the start.
///
/// **Four states in one case**: a day inside the span, the span's own end
/// day, a day genuinely outside the window on either side, and a
/// single-day fact with no span at all — any of the first three passing
/// alone is what a window widened until everything matches would also
/// produce, and the fourth is the regression the fix must not cause.
#[test]
fn a_happened_at_window_reaches_the_whole_span_not_only_its_start() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let spanning = Fact {
        happened_at: Some("2027-06-01".parse().expect("a civil date")),
        happened_through: Some("2027-06-10".parse().expect("a civil date")),
        ..fact(
            "person:milhouse",
            "f1",
            "the conference ran a stretch of June",
        )
    };
    let scanned = vec![doc(
        entity("person:milhouse", "Milhouse"),
        "His page.",
        vec![spanning],
    )];
    let asking = |day: &str| GraphQuery {
        select: Selection {
            near: Some(Nearness {
                day: day.parse().expect("a civil date"),
                within_days: 1,
                clock: Clock::HappenedAt,
            }),
            ..Selection::default()
        },
        include: Include {
            facts: true,
            ..GraphQuery::default().include
        },
        ..GraphQuery::default()
    };
    let found = |scanned: &[DocScan], day: &str| {
        resolve(scanned, &[], &asking(day))
            .expect("a read")
            .objects
            .iter()
            .flat_map(|o| o.facts.iter())
            .count()
    };

    assert_eq!(
        found(&scanned, "2027-06-05"),
        1,
        "a day squarely inside the recorded span was not found near it",
    );
    assert_eq!(
        found(&scanned, "2027-06-10"),
        1,
        "the span's own end day was not found near it",
    );
    assert_eq!(
        found(&scanned, "2027-06-11"),
        1,
        "a day just past the span's END, inside the window of that end, was measured from \
             the START instead and read as far away",
    );
    assert_eq!(
        found(&scanned, "2027-06-12"),
        0,
        "a day outside the span by more than the window came back anyway",
    );
    assert_eq!(
        found(&scanned, "2027-05-30"),
        0,
        "a day before the span by more than the window came back anyway",
    );

    let single_day = vec![doc(
        entity("person:homer", "Homer"),
        "His page.",
        vec![Fact {
            happened_at: Some("2027-06-01".parse().expect("a civil date")),
            ..fact("person:homer", "f2", "a claim about one day, no span")
        }],
    )];
    assert_eq!(
        found(&single_day, "2027-06-01"),
        1,
        "a single-day fact stopped being found on its own day",
    );
    assert_eq!(
        found(&single_day, "2027-06-12"),
        0,
        "a single-day fact was found eleven days from the only day it names",
    );
}

/// 🚨 **A day with nothing around it, and a clock that could not look, must
/// not read alike.**
///
/// The taken-in stamp is absent on every record written before it existed,
/// and it stays absent deliberately. So a read on that clock can come back
/// empty for two entirely different reasons: nothing was recorded near that
/// day, or nothing could be placed at all. **The count is what tells them
/// apart**, and without it the second silently reads as the first.
///
/// **Three states in one case**, because any two of them alone pass against
/// a build that has the third wrong.
#[test]
fn a_clock_that_cannot_place_a_record_counts_it_rather_than_dropping_it() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let scanned = vec![doc(
        entity("person:milhouse", "Milhouse"),
        "His page.",
        vec![
            fact("person:milhouse", "f1", "written before the stamp existed"),
            fact("person:milhouse", "f2", "also written before it"),
        ],
    )];
    let asking = |clock| GraphQuery {
        select: Selection {
            near: Some(Nearness {
                day: "2026-08-10".parse().expect("a civil date"),
                within_days: 7,
                clock,
            }),
            ..Selection::default()
        },
        include: Include {
            facts: true,
            ..GraphQuery::default().include
        },
        ..GraphQuery::default()
    };

    // The claim's own date places every record, so nothing is unreadable
    // and the records really are near the day.

    let by_day = resolve(&scanned, &[], &asking(Clock::RecordedOn)).expect("a read");
    assert_eq!(
        by_day.unplaced, 0,
        "the claim's own date is never absent, so nothing can be unplaceable on it",
    );
    assert_eq!(
        by_day.objects.iter().flat_map(|o| o.facts.iter()).count(),
        2,
        "the records are two days from the day asked about and did not come back",
    );

    // The same store on the other clock reaches nothing — and says so.
    let by_stamp = resolve(&scanned, &[], &asking(Clock::TakenIn)).expect("a read");
    assert_eq!(
        by_stamp.objects.iter().flat_map(|o| o.facts.iter()).count(),
        0,
        "a record with no stamp was placed on a clock that cannot place it",
    );
    assert_eq!(
        by_stamp.unplaced, 2,
        "the read came back empty and said nothing about what it could not look at, which \
             reads exactly like a day with nothing around it",
    );
}

/// 🚨 **The count answers the question that was asked, not the store.**
///
/// A caller who named a subject or a kind asked about those records. A
/// count taken over every document the walk happened to scan tells them
/// their own records could not be placed when every one of them was — and
/// because a walk with a near-day read scans every entity, one unrelated
/// stampless record anywhere makes zero unreachable forever.
///
/// **Both halves in one read, because the negative alone is satisfied by a
/// count that is always zero**: a selection whose own records are all
/// placeable reports none, and a selection whose own records cannot be
/// placed still reports them.
#[test]
fn the_unplaceable_count_is_narrowed_the_way_the_selection_is() {
    let _booted = crate::memory::testing::InMemoryMemory::booted();
    let stamped = |home: &str, id: &str, content: &str| Fact {
        inserted_at: Some("2026-08-09T12:00:00Z".parse().expect("a timestamp")),
        ..fact(home, id, content)
    };
    let scanned = vec![
        // Every record placeable on the taken-in clock, and near the day.
        doc(
            entity("person:milhouse", "Milhouse"),
            "His page.",
            vec![
                stamped("person:milhouse", "f1", "taken in the day before"),
                stamped("person:milhouse", "f2", "and so was this one"),
            ],
        ),
        // Unrelated, and written before the stamp existed — the backfilled
        // corpus a real instance is mostly made of.
        doc(
            entity("place:moes", "Moe's Tavern"),
            "The tavern's page.",
            vec![
                fact("place:moes", "f1", "written before the stamp existed"),
                fact("place:moes", "f2", "also written before it"),
            ],
        ),
    ];
    let asking = |select: Selection| GraphQuery {
        select: Selection {
            near: Some(Nearness {
                day: "2026-08-10".parse().expect("a civil date"),
                within_days: 7,
                clock: Clock::TakenIn,
            }),
            ..select
        },
        ..GraphQuery::default()
    };
    let read = |select: Selection| resolve(&scanned, &[], &asking(select)).expect("a read");
    let facts = |found: &Selected| found.objects.iter().flat_map(|o| o.facts.iter()).count();

    let his = read(Selection {
        subject: Some(EntityId("person:milhouse".to_string())),
        ..Selection::default()
    });
    assert_eq!(facts(&his), 2, "his records are placeable and near the day");
    assert_eq!(
        his.unplaced, 0,
        "every record he has was placed, so the read that named him has nothing to report \
             about records it could not look at",
    );

    let theirs = read(Selection {
        subject: Some(EntityId("place:moes".to_string())),
        ..Selection::default()
    });
    assert_eq!(facts(&theirs), 0, "no record of the tavern's can be placed");
    assert_eq!(
        theirs.unplaced, 2,
        "the tavern's own records are the ones this clock cannot look at, and a narrowed \
             count must still report them",
    );

    let people = read(Selection {
        kind: Some(EntityKind::PERSON),
        ..Selection::default()
    });
    assert_eq!(
        facts(&people),
        2,
        "the kind reaches the same placed records"
    );
    assert_eq!(
        people.unplaced, 0,
        "a kind narrows the count the way a handle does",
    );

    let places = read(Selection {
        kind: Some(EntityKind::PLACE),
        ..Selection::default()
    });
    assert_eq!(
        places.unplaced, 2,
        "the tavern drops out of the answer entirely because none of its records could be \
             placed, which is exactly why the count cannot be taken over the answer",
    );
}

/// A fact drawing one edge.
fn edged(home: &str, id: &str, content: &str, shape: EdgeShape, object: &str) -> Fact {
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
fn store() -> Vec<DocScan> {
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

fn handles(found: &[Object]) -> Vec<&str> {
    found.iter().map(|o| o.entity.id.as_str()).collect()
}

/// **A kind selects every object of it, and prose comes back whole.**
///
/// The acceptance case, in the general words that serve it: objects of a
/// kind, with their pages. Nothing here is a bot and nothing is a charter —
/// a charter IS an entity's prose, so the case falls out of `kind` and
/// `prose` rather than out of anything that knows what a charter is.
///
/// Paired with the negative it rests on: without `prose` the same query
/// answers `None`, so "the page came back" is a fact about this query and
/// not about every query.
#[test]
fn a_kind_selects_its_objects_and_prose_comes_back_whole() {
    let scanned = store();
    let query = GraphQuery {
        select: Selection {
            kind: Some(EntityKind::PERSON),
            ..Selection::default()
        },
        include: Include {
            facts: false,
            prose: true,
            stood_for: false,
        },
        follow: None,
        history: None,
    };
    let found = resolved(&scanned, &[], &query).expect("a kind is a selection");
    assert_eq!(
        handles(&found),
        vec![
            "person:barney-gumble",
            "person:ned-flanders",
            "person:ralph"
        ],
        "every person and nothing else, in handle order",
    );
    assert_eq!(
        found[1].prose.as_deref(),
        Some("Ned's page."),
        "the page comes back whole: {:?}",
        found[1],
    );
    assert!(
        found[1].facts.is_empty(),
        "facts were not asked for: {:?}",
        found[1],
    );

    let unasked = resolved(
        &scanned,
        &[],
        &GraphQuery {
            include: Include::default(),
            ..query.clone()
        },
    )
    .expect("the same selection");
    assert_eq!(
        unasked[1].prose, None,
        "prose that was not asked for is absent rather than empty: {:?}",
        unasked[1],
    );
}

/// **A shape's sources are left off the listing that also carries the
/// shape.** The shape already speaks for them in its own words, so
/// serving both would say the same thing twice. Three cases in one,
/// because each alone passes on a build that answers nothing: the count
/// says how many were left out, clearing the mark serves everything
/// again (the sabotage this case is built to catch), and asking for the
/// sources back with `stood_for: true` serves them beside the shape.
#[test]
fn a_shapes_sources_are_folded_out_of_the_facts_listing() {
    let leaf = fact("person:contract-graph-fold", "f1", "the first claim");
    let shape = Fact {
        stands_for: vec![FactAddress::new(
            EntityId("person:contract-graph-fold".into()),
            FactId("f1".into()),
        )],
        ..fact(
            "person:contract-graph-fold",
            "f2",
            "the newest claim, standing for the first",
        )
    };
    let query = GraphQuery {
        select: Selection {
            subject: Some(EntityId("person:contract-graph-fold".into())),
            ..Selection::default()
        },
        include: Include {
            facts: true,
            prose: false,
            stood_for: false,
        },
        follow: None,
        history: None,
    };
    fn ids(object: &Object) -> Vec<&str> {
        object.facts.iter().map(|f| f.id.as_str()).collect()
    }

    let marked = vec![doc(
        entity("person:contract-graph-fold", "Fold Case"),
        "The page.",
        vec![leaf.clone(), shape.clone()],
    )];
    let found = resolved(&marked, &[], &query).expect("a named subject always comes back");
    assert_eq!(
        ids(&found[0]),
        vec!["f2"],
        "the shape is served and its source is not: {:?}",
        found[0].facts,
    );
    assert_eq!(
        found[0].facts_folded, 1,
        "the count says how many were left out: {:?}",
        found[0],
    );
    assert_eq!(
        found[0].facts_held, 2,
        "the held count is the true total, unaffected by elision: {:?}",
        found[0],
    );

    // Sabotage: clear the mark and the same read must return everything —
    // a case that stayed green with the mark gone was never about the mark.
    let unmarked = vec![doc(
        entity("person:contract-graph-fold", "Fold Case"),
        "The page.",
        vec![
            leaf.clone(),
            Fact {
                stands_for: Vec::new(),
                ..shape.clone()
            },
        ],
    )];
    let found = resolved(&unmarked, &[], &query).expect("a named subject always comes back");
    assert_eq!(
        ids(&found[0]),
        vec!["f1", "f2"],
        "with the mark gone, the same read returns everything: {:?}",
        found[0].facts,
    );
    assert_eq!(found[0].facts_folded, 0);

    // The other argument: asking for the sources back serves both.
    let with_sources = GraphQuery {
        include: Include {
            stood_for: true,
            ..query.include
        },
        ..query.clone()
    };
    let found = resolved(&marked, &[], &with_sources).expect("a named subject always comes back");
    assert_eq!(
        ids(&found[0]),
        vec!["f1", "f2"],
        "stood_for: true serves the sources alongside the shape: {:?}",
        found[0].facts,
    );
    assert_eq!(
        found[0].facts_folded, 0,
        "nothing was left out when the sources were asked for: {:?}",
        found[0],
    );
}

/// **A record-scoped filter narrows which records come back — it must
/// not narrow how many the answer claims are behind them.**
///
/// `facts_held` is documented and already tested as "the true total,
/// unaffected by elision" (see the shape-fold case above). A record
/// filter is exactly as much an elision as shape-folding is: it decides
/// which of the object's own records are served, never how many exist.
/// An answer that let `facts_held` shrink to match a record filter would
/// be a smaller number that reads as "this is everything", which is
/// indistinguishable from an object that only ever held the one record.
#[test]
fn a_record_scoped_filter_narrows_what_comes_back_and_not_the_held_count() {
    let matching = Fact {
        fields: BTreeMap::from([("rsvp".to_string(), "yes".to_string())]),
        ..fact("person:milhouse", "f1", "said yes")
    };
    let other = Fact {
        fields: BTreeMap::from([("rsvp".to_string(), "no".to_string())]),
        ..fact("person:milhouse", "f2", "said no, a different sitting")
    };
    let scanned = vec![doc(
        entity("person:milhouse", "Milhouse"),
        "His page.",
        vec![matching, other],
    )];

    // The unscoped question: every record on the thing.
    let unscoped = GraphQuery {
        select: Selection {
            subject: Some(EntityId("person:milhouse".into())),
            ..Selection::default()
        },
        include: Include {
            facts: true,
            prose: false,
            stood_for: false,
        },
        follow: None,
        history: None,
    };
    let found = resolved(&scanned, &[], &unscoped).expect("a named subject always comes back");
    assert_eq!(
        found[0].facts.len(),
        2,
        "the unscoped question returns the full set: {:?}",
        found[0].facts,
    );
    assert_eq!(found[0].facts_held, 2);

    // The scoped question: only the record that answers the filter.
    let scoped = GraphQuery {
        select: Selection {
            fields: vec![FieldFilter::holding("rsvp", "yes").on_a_record()],
            ..unscoped.select.clone()
        },
        ..unscoped.clone()
    };
    let found = resolved(&scanned, &[], &scoped).expect("a named subject always comes back");
    assert_eq!(
        found[0]
            .facts
            .iter()
            .map(|f| f.id.as_str())
            .collect::<Vec<_>>(),
        vec!["f1"],
        "the scoped question narrows what comes back: {:?}",
        found[0].facts,
    );
    assert_eq!(
        found[0].facts_held, 2,
        "the held count must still say the true total — the caller's own filter is not the \
             store's whole picture of the thing: {:?}",
        found[0],
    );
}

/// **A key and its value select the objects holding such a record**, and
/// the value half is what makes it a filter rather than a key check.
///
/// Both directions in one case: the same key with the other value selects
/// the other object. A build that matched on the key alone passes the
/// first assertion and fails the second.
#[test]
fn a_key_and_its_value_select_the_objects_that_hold_it() {
    let scanned = store();
    let by_value = |value: &str| {
        let query = GraphQuery {
            select: Selection {
                fields: vec![FieldFilter::holding("rsvp", value)],
                ..Selection::default()
            },
            ..GraphQuery::default()
        };
        resolved(&scanned, &[], &query).expect("a key filter is a selection")
    };
    assert_eq!(handles(&by_value("yes")), vec!["person:ralph"]);
    assert_eq!(handles(&by_value("no")), vec!["person:barney-gumble"]);

    let any = resolved(
        &scanned,
        &[],
        &GraphQuery {
            select: Selection {
                fields: vec![FieldFilter::key("rsvp")],
                ..Selection::default()
            },
            ..GraphQuery::default()
        },
    )
    .expect("a key alone is a selection");
    assert_eq!(
        handles(&any),
        vec!["person:barney-gumble", "person:ralph"],
        "the key with no value asked for holds for either value",
    );
    // **A filter says which OBJECTS, and by default it says nothing about
    // which of an object's records come back.** Ralph holds two, one of
    // which carries no key at all, and both are hers.
    assert_eq!(
        any[1].facts.len(),
        2,
        "a selection filter does not narrow the object's page: {:?}",
        any[1],
    );

    // **Asked of a record, the same filter also narrows the page** — the
    // other question, and the pair is what says the two are apart. Without
    // the assertion above, this one passes on a build where every filter is
    // still the record's.
    let on_a_record = resolved(
        &scanned,
        &[],
        &GraphQuery {
            select: Selection {
                fields: vec![FieldFilter::key("rsvp").on_a_record()],
                ..Selection::default()
            },
            ..GraphQuery::default()
        },
    )
    .expect("a key filter asked of a record is a selection too");
    assert_eq!(
        handles(&on_a_record),
        vec!["person:barney-gumble", "person:ralph"],
        "the same objects: each holds a record carrying the key",
    );
    assert_eq!(
        on_a_record[1].facts.len(),
        1,
        "and it comes back with the record that answered, not its whole page: {:?}",
        on_a_record[1],
    );
}

/// 🚨 **The two scopes disagree about WHICH OBJECTS when the fold and a
/// record disagree about the value.**
///
/// The cases above ask both scopes of a store where every key was written
/// once, so both select the same object and the only difference on show is
/// which records ride along. **That leaves the load-bearing half untested:
/// a key written twice, where the thing no longer holds what a record of it
/// still says.**
///
/// Asked of the THING, the old value selects nothing — the newest write won
/// the fold. Asked of a RECORD, it selects, because a record did carry it.
/// **A caller that reads one as the other writes a filter that goes empty
/// the moment anybody rewrites the key.**
///
/// The positive is the third read: the thing IS selected by what it holds
/// now, so the negative above is a fact about the value rather than about a
/// store nothing can be found in.
#[test]
fn the_scopes_part_company_when_the_fold_and_a_record_disagree() {
    let wrote = |id: &str, content: &str, rsvp: &str| Fact {
        fields: [("rsvp".to_string(), rsvp.to_string())]
            .into_iter()
            .collect(),
        ..fact("person:ralph", id, content)
    };
    // Two writes of one key, and the thing holds the newer of them. The
    // fields are stated rather than folded from the rows, because that is
    // what the store does and a fixture that recomputed it could not put
    // the two out of step.
    let scanned = vec![doc_holding(
        entity("person:ralph", "Ralph"),
        "Ralph's page.",
        vec![
            wrote("f1", "coming to the party", "yes"),
            wrote("f2", "cannot make it after all", "no"),
        ],
        [("rsvp".to_string(), "no".to_string())]
            .into_iter()
            .collect(),
    )];
    let by = |filter: FieldFilter| {
        resolved(
            &scanned,
            &[],
            &GraphQuery {
                select: Selection {
                    fields: vec![filter],
                    ..Selection::default()
                },
                ..GraphQuery::default()
            },
        )
        .expect("a key filter is a selection")
    };

    assert!(
        handles(&by(FieldFilter::holding("rsvp", "yes"))).is_empty(),
        "the thing does not hold the value it used to, so asked of the thing it is not \
             selected",
    );
    assert_eq!(
        handles(&by(FieldFilter::holding("rsvp", "yes").on_a_record())),
        vec!["person:ralph"],
        "asked of a record, the write that happened still answers",
    );
    assert_eq!(
        handles(&by(FieldFilter::holding("rsvp", "no"))),
        vec!["person:ralph"],
        "and the thing is selected by what it holds now, so the empty answer above is about \
             the value rather than about an unreachable store",
    );
}

/// **A filter and a kind combine into one set**, rather than being two
/// questions asked in turn.
#[test]
fn a_kind_and_a_key_narrow_one_set() {
    let scanned = store();
    let query = |kind: EntityKind| GraphQuery {
        select: Selection {
            kind: Some(kind),
            fields: vec![FieldFilter::key("rsvp")],
            ..Selection::default()
        },
        ..GraphQuery::default()
    };
    assert_eq!(
        handles(&resolved(&scanned, &[], &query(EntityKind::PERSON)).expect("both filters")),
        vec!["person:barney-gumble", "person:ralph"],
    );
    assert!(
        resolved(&scanned, &[], &query(EntityKind::PLACE))
            .expect("both filters")
            .is_empty(),
        "the kind narrows the same set the key does, so a kind holding no such record is empty",
    );
}

/// **A type selects structurally**, over the keys a record carries and
/// never over what anybody declared it to be.
#[test]
fn a_type_selects_the_records_that_answer_it() {
    let scanned = store();
    let declared = types::DeclaredType::new(
        "reply",
        vec![types::Field::required("rsvp", types::ValueType::Text)],
    );
    let found = resolved(
        &scanned,
        &[],
        &GraphQuery {
            select: Selection {
                answers_type: Some(declared),
                ..Selection::default()
            },
            ..GraphQuery::default()
        },
    )
    .expect("a type is a selection");
    assert_eq!(
        handles(&found),
        vec!["person:barney-gumble", "person:ralph"],
    );

    let unheld = types::DeclaredType::new(
        "shipment",
        vec![types::Field::required("weight", types::ValueType::Number)],
    );
    assert!(
        resolved(
            &scanned,
            &[],
            &GraphQuery {
                select: Selection {
                    answers_type: Some(unheld),
                    ..Selection::default()
                },
                ..GraphQuery::default()
            },
        )
        .expect("a type is a selection")
        .is_empty(),
        "a type no record answers selects nothing — or 'it matched' says nothing",
    );
}

/// **A THING answers a type, across everything recorded about it.**
///
/// The question is not whether one record carries the type's keys. It is
/// whether the thing does, and a thing's fields are its records' fields
/// folded into one. Asking it of a record makes a thing that was described
/// over two sittings answer nothing, which is how most things get written
/// down.
#[test]
fn a_thing_answers_a_type_its_records_answer_only_together() {
    let declared = types::DeclaredType::new(
        "crate",
        vec![
            types::Field::required("weight", types::ValueType::Number),
            types::Field::required("arrives", types::ValueType::Date),
        ],
    );
    // One key each, and neither record answers the type on its own.
    let halves = |home: &str| {
        vec![
            Fact {
                fields: [("weight".to_string(), "12".to_string())]
                    .into_iter()
                    .collect(),
                ..fact(home, "f1", "somebody weighed it")
            },
            Fact {
                fields: [("arrives".to_string(), "2026-08-10".to_string())]
                    .into_iter()
                    .collect(),
                ..fact(home, "f2", "and somebody else was told when")
            },
        ]
    };
    let scanned = vec![
        doc(
            entity("thing:folding-chairs", "The Folding Chairs"),
            "",
            halves("thing:folding-chairs"),
        ),
        // The negative: a thing carrying one of the keys and no more. It is
        // what stops "the fold reached it" from meaning "everything comes
        // back".
        doc(
            entity("thing:torque-wrench", "The Torque Wrench"),
            "",
            vec![Fact {
                fields: [("weight".to_string(), "3".to_string())]
                    .into_iter()
                    .collect(),
                ..fact("thing:torque-wrench", "f1", "a lighter thing")
            }],
        ),
    ];

    let found = resolved(
        &scanned,
        &[],
        &GraphQuery {
            select: Selection {
                answers_type: Some(declared),
                ..Selection::default()
            },
            ..GraphQuery::default()
        },
    )
    .expect("a type is a selection");
    assert_eq!(
        handles(&found),
        vec!["thing:folding-chairs", "thing:torque-wrench"],
        "both answer; the fold is what lets the first one answer at all"
    );
    // …and the object says how it answers, which is the thing the caller
    // asked for. Reported at the object, because that is the unit the
    // question was asked of.
    let whole = found
        .iter()
        .find(|o| o.entity.id.as_str() == "thing:folding-chairs")
        .expect("the whole one is in the answer");
    let answered = whole.answers.as_ref().expect("it answered a type");
    assert!(
        answered.complete(),
        "two records between them hold every key: {answered:?}"
    );
    let partial = found
        .iter()
        .find(|o| o.entity.id.as_str() == "thing:torque-wrench")
        .expect("the partial one is in the answer");
    assert_eq!(
        partial
            .answers
            .as_ref()
            .expect("it answered a type")
            .lacking,
        vec!["arrives".to_string()],
        "a partial answer names what it lacks rather than being dropped"
    );
}

/// **The type question is asked of what the THING holds, never of the
/// records under it.**
///
/// A key written twice is not a conflict to report — it is the key being
/// written twice, and which write won is the store's answer, not a walk's
/// to work out. So the fixture hands over a thing whose standing value is
/// the newer one while both records are still there, and the walk must read
/// the standing value: a walk that folded the records for itself would find
/// the other one.
///
/// Read through the declared value type, because that is where a folded
/// VALUE surfaces: the value the thing holds is not a number and is
/// reported as mistyped, where the one it replaced would have passed
/// silently.
#[test]
fn the_type_question_reads_what_the_thing_holds() {
    let declared = types::DeclaredType::new(
        "crate",
        vec![types::Field::required("weight", types::ValueType::Number)],
    );
    let weighed = |id: &str, weight: &str| Fact {
        fields: [("weight".to_string(), weight.to_string())]
            .into_iter()
            .collect(),
        ..fact("thing:bike-chain", id, "somebody weighed it")
    };
    let scanned = vec![doc_holding(
        entity("thing:bike-chain", "The Chain"),
        "",
        // Out of order on purpose: the records say nothing about which
        // write won, and the walk must not read them as if they did.
        vec![
            weighed("f2", "heavier than the last one"),
            weighed("f1", "12"),
        ],
        [(
            "weight".to_string(),
            "heavier than the last one".to_string(),
        )]
        .into_iter()
        .collect(),
    )];

    let found = resolved(
        &scanned,
        &[],
        &GraphQuery {
            select: Selection {
                answers_type: Some(declared),
                ..Selection::default()
            },
            ..GraphQuery::default()
        },
    )
    .expect("a type is a selection");
    let answered = found
        .first()
        .expect("the thing answers the type either way")
        .answers
        .as_ref()
        .expect("it answered a type");
    assert_eq!(
        answered
            .mistyped
            .iter()
            .map(|m| m.value.as_str())
            .collect::<Vec<_>>(),
        vec!["heavier than the last one"],
        "the type was asked of the value the thing holds: {answered:?}"
    );
}

/// **The walk goes both ways, and the answer nests.**
///
/// **A walk says when the claim behind a link was taken back.**
///
/// A fact read can already tell a retracted claim from one that never
/// existed: the claim comes back carrying its status, marked rather than
/// hidden. A walk could not, so the retracted attendance and the live one
/// arrived by the same edge, indistinguishable.
///
/// **Marked, never filtered.** Dropping the link would make a claim
/// somebody took back and a claim nobody ever made identical on a walk,
/// which is the failure this exists to remove rather than a tidier version
/// of it.
///
/// **All three reads in one case.** The live link unmarked is what stops
/// the marker being on everything; the retracted one marked is the
/// capability; and the person nobody ever linked is absent from both, which
/// is what stops the pair passing on a store where the walk reaches
/// everybody.
#[test]
fn a_walk_marks_a_link_whose_claim_was_taken_back() {
    let mut scanned = store();
    let barney = scanned
        .iter_mut()
        .find(|d| d.doc_id == "person:barney-gumble")
        .expect("the store holds Barney");
    barney.facts[0].status = FactStatus::Archived;

    let guests = resolved(
        &scanned,
        &[],
        &GraphQuery {
            select: Selection {
                subject: Some(EntityId("event:birthday-party".into())),
                ..Selection::default()
            },
            follow: Some(Follow {
                along: Along::Edge(EdgeShape::Attendance),
                direction: Some(Direction::In),
                ..Follow::hop()
            }),
            ..GraphQuery::default()
        },
    )
    .expect("a subject with a walk");
    let reached = &guests[0].connected;

    assert_eq!(
        handles(reached),
        vec!["person:barney-gumble", "person:ralph"],
        "both are still reached, because a retracted claim is marked and not hidden: \
             {reached:?}",
    );
    let via = |at: usize| {
        reached[at]
            .via
            .as_ref()
            .expect("a reached object says how the walk got to it")
    };
    assert!(
        via(0).retracted,
        "Barney's attendance was taken back, so the link says so: {reached:?}",
    );
    assert!(
        !via(1).retracted,
        "Ralph's still stands, so nothing marks hers — without this the marker could be on \
             every link: {reached:?}",
    );
    assert_eq!(
        via(0).link,
        via(1).link,
        "and both arrived by the same edge, so the marker is the only difference: {reached:?}",
    );
    assert!(
        !handles(reached).contains(&"person:ned-flanders"),
        "somebody no claim ever linked is not reached at all, so the pair above is about the \
             claims rather than about a walk that returns everybody: {reached:?}",
    );

    // **The other end of the same edge**, because a walk outbound reads the
    // object's own records while inbound reads a map of who points here.
    // They are different code, so a marker on one says nothing about the
    // other — and the caller asking *what was this person at* is walking
    // outbound.
    let out_from = |who: &str| {
        resolved(
            &scanned,
            &[],
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId(who.into())),
                    ..Selection::default()
                },
                follow: Some(Follow {
                    along: Along::Edge(EdgeShape::Attendance),
                    direction: Some(Direction::Out),
                    ..Follow::hop()
                }),
                ..GraphQuery::default()
            },
        )
        .expect("a subject with a walk")[0]
            .connected
            .clone()
    };
    let withdrawn = out_from("person:barney-gumble");
    let stands = out_from("person:ralph");
    assert_eq!(
        handles(&withdrawn),
        vec!["event:birthday-party"],
        "the party is still reached from the guest whose claim went: {withdrawn:?}",
    );
    assert!(
        withdrawn[0].via.as_ref().is_some_and(|v| v.retracted),
        "and walking out says the claim behind it was taken back: {withdrawn:?}",
    );
    assert!(
        stands[0].via.as_ref().is_some_and(|v| !v.retracted),
        "while the guest whose claim stands reaches it unmarked: {stands:?}",
    );
}

/// A corpus with one target reached by all three routes at once: a claim
/// drawing an edge, a claim mentioning the target in its own words, and a
/// claim naming it in `refs` — nothing else in common between the three.
fn three_routes_to_one_target() -> Vec<DocScan> {
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

/// **An unscoped walk reaches a mention and a ref beside an edge, each
/// labelled with how it got there** — the shape `Along::AnyEdge` exists
/// for, once a mention or a ref can answer it too.
#[test]
fn an_unscoped_walk_in_reaches_the_edge_the_mention_and_the_ref() {
    let scanned = three_routes_to_one_target();
    let found = resolved(
        &scanned,
        &[],
        &GraphQuery {
            select: Selection {
                subject: Some(EntityId("topic:all-rules".into())),
                ..Selection::default()
            },
            follow: Some(Follow {
                along: Along::AnyEdge,
                direction: Some(Direction::In),
                ..Follow::hop()
            }),
            ..GraphQuery::default()
        },
    )
    .expect("a subject with a walk");
    let reached = &found[0].connected;

    assert_eq!(
        handles(reached),
        vec!["person:gayle", "person:homer", "person:hugo"],
        "all three routes are reached, and nothing else is: {reached:?}",
    );
    let via_of = |handle: &str| {
        reached
            .iter()
            .find(|o| o.entity.id.as_str() == handle)
            .expect("the handle is among the reached")
            .via
            .as_ref()
            .expect("a reached object says how the walk got to it")
    };
    assert_eq!(
        via_of("person:homer").link,
        Link::Edge(EdgeShape::Connection),
        "the edge writer is labelled as an edge: {reached:?}",
    );
    assert_eq!(
        via_of("person:gayle").link,
        Link::Mention,
        "the mention writer is labelled as a mention, never as an edge: {reached:?}",
    );
    assert_eq!(
        via_of("person:hugo").link,
        Link::Ref,
        "the ref writer is labelled as a ref: {reached:?}",
    );
}

/// **A shaped walk answers exactly as it always has** — a mention and a
/// ref have no shape, so they never answer one, and a walk that names an
/// edge shape must not start returning them as an accidental side effect
/// of the map that now exists beside `inbound`.
#[test]
fn a_walk_scoped_to_one_edge_shape_never_returns_a_mention_or_a_ref() {
    let scanned = three_routes_to_one_target();
    let found = resolved(
        &scanned,
        &[],
        &GraphQuery {
            select: Selection {
                subject: Some(EntityId("topic:all-rules".into())),
                ..Selection::default()
            },
            follow: Some(Follow {
                along: Along::Edge(EdgeShape::Connection),
                direction: Some(Direction::In),
                ..Follow::hop()
            }),
            ..GraphQuery::default()
        },
    )
    .expect("a subject with a walk");
    let reached = &found[0].connected;

    assert_eq!(
        handles(reached),
        vec!["person:homer"],
        "only the edge claim answers a walk scoped to its own shape: {reached:?}",
    );
}

/// **The outbound mirror**: from the subject whose claim mentions the
/// target, an unscoped walk out reaches it, labelled as a mention.
#[test]
fn an_unscoped_walk_out_reaches_the_target_of_its_own_mention() {
    let scanned = three_routes_to_one_target();
    let found = resolved(
        &scanned,
        &[],
        &GraphQuery {
            select: Selection {
                subject: Some(EntityId("person:gayle".into())),
                ..Selection::default()
            },
            follow: Some(Follow {
                along: Along::AnyEdge,
                direction: Some(Direction::Out),
                ..Follow::hop()
            }),
            ..GraphQuery::default()
        },
    )
    .expect("a subject with a walk");
    let reached = &found[0].connected;

    assert_eq!(
        handles(reached),
        vec!["topic:all-rules"],
        "the mentioned target is reached going out: {reached:?}",
    );
    assert_eq!(
        reached[0]
            .via
            .as_ref()
            .expect("a reached object says how the walk got to it")
            .link,
        Link::Mention,
        "labelled as a mention: {reached:?}",
    );
}

/// A corpus where a handle sits in a field value under a key nobody
/// declared, and a list of handles sits under another, beside three values
/// that are not links: a handle nothing answers to, a handle inside a longer
/// string, and a list with one item that is not a handle.
fn field_holders() -> Vec<DocScan> {
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

/// **A field holding a handle is a link whatever its key, and so is a list of
/// handles** (decision log 374: a link is any field that names another thing).
/// An unscoped walk in reaches the record whose value IS the handle and the one
/// whose value is a list of handles, under keys no type declared, labelled as
/// field links. The three values beside them that only look like links are not
/// reached — the positives keep the three negatives from passing over an empty
/// answer, and the list with one item that is not a handle stays prose.
#[test]
fn an_unscoped_walk_in_reaches_a_handle_held_under_an_undeclared_key() {
    let scanned = field_holders();
    let found = resolved(
        &scanned,
        &[],
        &GraphQuery {
            select: Selection {
                subject: Some(EntityId("topic:all-rules".into())),
                ..Selection::default()
            },
            follow: Some(Follow {
                along: Along::AnyEdge,
                direction: Some(Direction::In),
                ..Follow::hop()
            }),
            ..GraphQuery::default()
        },
    )
    .expect("a subject with a walk");
    let reached = &found[0].connected;

    assert_eq!(
        handles(reached),
        vec!["person:homer", "person:ned-flanders"],
        "a value that is a handle, and a list that is only handles, are links: {reached:?}",
    );
    for one in reached {
        assert_eq!(
            one.via
                .as_ref()
                .expect("a reached object says how the walk got to it")
                .link,
            Link::Field,
            "labelled as a field link, never as an edge: {reached:?}",
        );
    }

    // The second handle of the list is reached too, not only the first.
    let widgets = resolved(
        &scanned,
        &[],
        &GraphQuery {
            select: Selection {
                subject: Some(EntityId("topic:widgets".into())),
                ..Selection::default()
            },
            follow: Some(Follow {
                along: Along::AnyEdge,
                direction: Some(Direction::In),
                ..Follow::hop()
            }),
            ..GraphQuery::default()
        },
    )
    .expect("a subject with a walk");
    assert_eq!(
        handles(&widgets[0].connected),
        vec!["person:ned-flanders"],
        "{:?}",
        widgets[0].connected,
    );
}

/// **The outbound mirror**: from the record holding the handle, an unscoped
/// walk out reaches the thing it names, and a handle nothing answers to is
/// dropped by the existence retain like every other branch.
#[test]
fn an_unscoped_walk_out_reaches_a_handle_held_under_an_undeclared_key() {
    let scanned = field_holders();
    let out_of = |who: &str| {
        resolved(
            &scanned,
            &[],
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId(who.into())),
                    ..Selection::default()
                },
                follow: Some(Follow {
                    along: Along::AnyEdge,
                    direction: Some(Direction::Out),
                    ..Follow::hop()
                }),
                ..GraphQuery::default()
            },
        )
        .expect("a subject with a walk")[0]
            .connected
            .clone()
    };
    let homer = out_of("person:homer");
    assert_eq!(handles(&homer), vec!["topic:all-rules"], "{homer:?}");
    assert_eq!(
        homer[0].via.as_ref().expect("a via").link,
        Link::Field,
        "{homer:?}",
    );
    let gayle = out_of("person:gayle");
    assert!(
        gayle.is_empty(),
        "a handle that resolves to nothing is not a link: {gayle:?}"
    );
    let ned = out_of("person:ned-flanders");
    assert_eq!(
        handles(&ned),
        vec!["topic:all-rules", "topic:widgets"],
        "every handle in a list of handles is reached: {ned:?}"
    );
    let barney = out_of("person:barney-gumble");
    assert!(
        barney.is_empty(),
        "a list with an item that is not a handle is prose: {barney:?}"
    );
}

/// **A walk scoped to an edge shape or a relation never returns a field
/// link** — naming a shape asks for that shape, as it does for a mention.
#[test]
fn a_walk_scoped_to_one_edge_shape_never_returns_a_field_link() {
    let scanned = field_holders();
    let found = resolved(
        &scanned,
        &[],
        &GraphQuery {
            select: Selection {
                subject: Some(EntityId("topic:all-rules".into())),
                ..Selection::default()
            },
            follow: Some(Follow {
                along: Along::Edge(EdgeShape::Connection),
                direction: Some(Direction::In),
                ..Follow::hop()
            }),
            ..GraphQuery::default()
        },
    )
    .expect("a subject with a walk");
    assert!(
        found[0].connected.is_empty(),
        "a shaped walk reaches only its shape: {:?}",
        found[0].connected,
    );
}

/// **Two claims can draw one link, and a claim that stands keeps it.**
///
/// The marker says *nothing stands behind this*, so it must not fire
/// because one of several records happened to be taken back. Barney is
/// recorded twice, once withdrawn and once not, and arrives ONCE by an
/// unmarked link — the same answer as if the withdrawn record had never
/// been written, which is correct: it is not what the link rests on.
#[test]
fn a_claim_that_stands_keeps_a_link_another_claim_gave_up() {
    let mut scanned = store();
    let barney = scanned
        .iter_mut()
        .find(|d| d.doc_id == "person:barney-gumble")
        .expect("the store holds Barney");
    barney.facts[0].status = FactStatus::Archived;
    barney.facts.push(Fact {
        edge: Some(Edge {
            shape: EdgeShape::Attendance,
            object: EntityId("event:birthday-party".into()),
        }),
        ..fact("person:barney-gumble", "f2", "came after all")
    });

    let reached = resolved(
        &scanned,
        &[],
        &GraphQuery {
            select: Selection {
                subject: Some(EntityId("event:birthday-party".into())),
                ..Selection::default()
            },
            follow: Some(Follow {
                along: Along::Edge(EdgeShape::Attendance),
                direction: Some(Direction::In),
                ..Follow::hop()
            }),
            ..GraphQuery::default()
        },
    )
    .expect("a subject with a walk")[0]
        .connected
        .clone();

    assert_eq!(
        handles(&reached),
        vec!["person:barney-gumble", "person:ralph"],
        "one link each, not one per claim: {reached:?}",
    );
    assert!(
        !reached[0]
            .via
            .as_ref()
            .expect("a reached object says how the walk got to it")
            .retracted,
        "a claim that stands draws the link, so the withdrawn one does not mark it: \
             {reached:?}",
    );
}

/// From the party inbound reaches its guests; from a guest outbound reaches
/// the party. One edge, two questions, and a walk that could only go one
/// way would answer one of them.
#[test]
fn a_walk_leaves_by_either_end_of_an_edge() {
    let scanned = store();
    let from_party = |direction: Direction| {
        resolved(
            &scanned,
            &[],
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId("event:birthday-party".into())),
                    ..Selection::default()
                },
                include: Include {
                    facts: false,
                    prose: false,
                    stood_for: false,
                },
                follow: Some(Follow {
                    along: Along::Edge(EdgeShape::Attendance),
                    direction: Some(direction),
                    depth: 1,
                    keeping: Vec::new(),
                    fits_type: None,
                }),
                history: None,
            },
        )
        .expect("a subject with a walk")
    };

    let inbound = from_party(Direction::In);
    assert_eq!(
        handles(&inbound[0].connected),
        vec!["person:barney-gumble", "person:ralph"],
        "the guests hang off the party: {:?}",
        inbound[0],
    );
    assert_eq!(
        inbound[0].connected[0].via,
        Some(Via {
            link: Link::Edge(EdgeShape::Attendance),
            direction: Direction::In,
            retracted: false,
        }),
        "a reached object says how the walk got to it",
    );
    assert_eq!(inbound[0].via, None, "a root was reached by nothing");

    assert!(
        from_party(Direction::Out)[0].connected.is_empty(),
        "the party's own record draws no edge, so outbound reaches nobody",
    );

    let from_guest = resolved(
        &scanned,
        &[],
        &GraphQuery {
            select: Selection {
                subject: Some(EntityId("person:ralph".into())),
                ..Selection::default()
            },
            follow: Some(Follow::hop()),
            ..GraphQuery::default()
        },
    )
    .expect("a subject with a walk");
    assert_eq!(
        handles(&from_guest[0].connected),
        vec!["event:birthday-party"],
        "and outbound from the guest reaches the party",
    );
}

/// **The question a flat list cannot answer, in one call**: what do my
/// guests eat. One hop from the party reaches the guests, and each of them
/// arrives carrying their own page.
///
/// The load-bearing half is that a reached object brings ITS facts, not
/// the root's — a walk that returned bare handles would leave the caller
/// asking again per guest, which is two calls for one question.
#[test]
fn a_reached_object_brings_its_own_facts() {
    let found = resolved(
        &store(),
        &[],
        &GraphQuery {
            select: Selection {
                subject: Some(EntityId("event:birthday-party".into())),
                ..Selection::default()
            },
            include: Include::default(),
            follow: Some(Follow {
                along: Along::Edge(EdgeShape::Attendance),
                direction: Some(Direction::In),
                depth: 1,
                keeping: Vec::new(),
                fits_type: None,
            }),
            history: None,
        },
    )
    .expect("a walk of one hop");

    let guests = &found[0].connected;
    assert_eq!(
        handles(guests),
        vec!["person:barney-gumble", "person:ralph"],
    );
    let ralph = guests
        .iter()
        .find(|o| o.entity.id.as_str() == "person:ralph")
        .expect("the guest who is coming");
    assert!(
        ralph.facts.iter().any(|f| f.content == "vegetarian"),
        "the guest arrives with her own page: {ralph:?}",
    );
    assert!(
        found[0]
            .facts
            .iter()
            .any(|f| f.content == "starts at eight"),
        "and the root still carries its own: {:?}",
        found[0],
    );
}

/// **A second hop is a second hop.** From a guest, to the party she is
/// attending, to the place it is held — three objects nested two deep, in
/// one call.
///
/// Paired with the same walk one hop shallower, which reaches the party
/// and stops. Without that negative the case passes on a build that walks
/// forever and on one that ignores `depth` entirely.
#[test]
fn a_two_hop_walk_returns_the_shape_and_not_a_list() {
    let scanned = store();
    let from_ralph = |depth: usize| {
        resolved(
            &scanned,
            &[],
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId("person:ralph".into())),
                    ..Selection::default()
                },
                include: Include {
                    facts: false,
                    prose: false,
                    stood_for: false,
                },
                follow: Some(Follow {
                    along: Along::AnyEdge,
                    direction: Some(Direction::Out),
                    depth,
                    keeping: Vec::new(),
                    fits_type: None,
                }),
                history: None,
            },
        )
        .expect("a walk out of a subject")
    };

    let two = from_ralph(2);
    assert_eq!(handles(&two[0].connected), vec!["event:birthday-party"]);
    assert_eq!(
        handles(&two[0].connected[0].connected),
        vec!["place:moes"],
        "the second hop reaches where the party is held: {two:?}",
    );
    assert_eq!(
        two[0].connected[0].connected[0].via,
        Some(Via {
            link: Link::Edge(EdgeShape::Location),
            direction: Direction::Out,
            retracted: false,
        }),
        "and it says which edge it came along",
    );

    assert!(
        from_ralph(1)[0].connected[0].connected.is_empty(),
        "one hop stops at the party: {:?}",
        from_ralph(1),
    );
}

/// **A walk that stopped says so, and one that ran out of graph does
/// not.** An empty `connected` otherwise means two different things —
/// the hops ran out, or there is nothing there — and a reader who has to
/// infer which will eventually infer wrong.
///
/// Three states in one case, because the flag is only worth anything if it
/// is off when it should be: cut short at the ceiling, cut short by a
/// neighbour already in the answer, and genuinely at the end of the graph.
#[test]
fn an_object_says_when_it_has_edges_nobody_followed() {
    let scanned = store();
    let from_ralph = |depth: usize| {
        resolved(
            &scanned,
            &[],
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId("person:ralph".into())),
                    ..Selection::default()
                },
                include: Include {
                    facts: false,
                    prose: false,
                    stood_for: false,
                },
                follow: Some(Follow {
                    along: Along::AnyEdge,
                    direction: Some(Direction::Out),
                    depth,
                    keeping: Vec::new(),
                    fits_type: None,
                }),
                history: None,
            },
        )
        .expect("a walk out of a subject")
    };

    let one = from_ralph(1);
    let party = &one[0].connected[0];
    assert!(
        party.connected.is_empty() && party.unwalked,
        "the party was reached on the last hop and its own edges were not followed: {party:?}",
    );
    assert!(
        !one[0].unwalked,
        "and the root's one edge WAS followed, so nothing was left out there: {:?}",
        one[0],
    );

    let two = from_ralph(2);
    let party = &two[0].connected[0];
    assert!(
        party.unwalked,
        "at two hops the party still points back at Ralph, who is already in the answer: \
             {party:?}",
    );
    assert!(
        !party.connected[0].unwalked,
        "and the tavern draws no edge at all, so nothing was left out there: {party:?}",
    );
}

/// **A cycle stops.** Ralph attends the party and the party points back
/// at Ralph, so an outbound walk at the ceiling would loop forever if
/// nothing held what it had already reached.
///
/// It asserts where the walk actually ends rather than only that it
/// returned: a build that stopped after one hop for the wrong reason would
/// satisfy "it terminated".
#[test]
fn a_walk_over_a_cycle_terminates() {
    let found = resolved(
        &store(),
        &[],
        &GraphQuery {
            select: Selection {
                subject: Some(EntityId("person:ralph".into())),
                ..Selection::default()
            },
            include: Include {
                facts: false,
                prose: false,
                stood_for: false,
            },
            follow: Some(Follow {
                along: Along::AnyEdge,
                direction: Some(Direction::Out),
                depth: MAX_DEPTH,
                keeping: Vec::new(),
                fits_type: None,
            }),
            history: None,
        },
    )
    .expect("a walk at the ceiling");

    let party = &found[0].connected;
    assert_eq!(handles(party), vec!["event:birthday-party"]);
    assert_eq!(
        handles(&party[0].connected),
        vec!["place:moes"],
        "the party points back at Ralph and at the tavern; only the tavern is new: {found:?}",
    );
    assert!(
        party[0].connected[0].connected.is_empty(),
        "and the tavern draws no edge, so the walk ends there: {found:?}",
    );
}

/// **A named subject comes back even when the filters keep none of its
/// facts** — naming a handle asks for that object, and a filter asks which
/// objects. Paired with the miss it must not be confused with.
#[test]
fn a_named_subject_is_not_filtered_away_and_an_unknown_one_is_a_miss() {
    let scanned = store();
    let found = resolved(
        &scanned,
        &[],
        &GraphQuery {
            select: Selection {
                subject: Some(EntityId("person:ned-flanders".into())),
                // Asked of a record, so the object is the one thing that
                // brings it back and the filter decides only which of its
                // records ride along.
                fields: vec![FieldFilter::holding("rsvp", "yes").on_a_record()],
                ..Selection::default()
            },
            ..GraphQuery::default()
        },
    )
    .expect("a named subject");
    assert_eq!(handles(&found), vec!["person:ned-flanders"]);
    assert!(
        found[0].facts.is_empty(),
        "and it comes back with the records that answered, which is none: {:?}",
        found[0],
    );

    let missed = resolved(
        &scanned,
        &[],
        &GraphQuery::subject(EntityId("person:ned-flander".into())),
    )
    .expect_err("a handle that names nothing is a miss");
    match missed {
        MemoryError::UnknownEntity { attempted, nearest } => {
            assert_eq!(attempted, "person:ned-flander");
            assert_eq!(
                nearest.first().map(|n| n.handle.as_str()),
                Some("person:ned-flanders"),
                "the near candidate comes back: {nearest:?}",
            );
        }
        other => panic!("a miss must name the handle and its candidates: {other:?}"),
    }
}

/// **The narrows-nothing refusal names every way forward, including the
/// two a caller reaches by an address alone** — `history_record` and
/// `built_on` each select without a subject, by filling one in from the
/// address's own home entity, so a refusal that omits them from its list
/// of options misleads a caller into thinking a subject, a kind, a type
/// or a key are the only ways to select something.
#[test]
fn the_narrows_nothing_refusal_names_a_record_address_as_a_way_forward() {
    let refused = resolved(&store(), &[], &GraphQuery::default())
        .expect_err("an empty query cannot be served");
    let message = refused.to_string();
    assert!(message.contains("history_record"), "{message}");
    assert!(message.contains("built_on"), "{message}");
}

/// **A query that narrows nothing is refused**, and so is a walk of no
/// hops and one past the ceiling. Each is a malformed argument rather than
/// an unlucky one, so each reads as the caller's mistake instead of as an
/// honest empty answer.
#[test]
fn a_query_that_narrows_nothing_is_refused() {
    let refused = |query: GraphQuery| {
        resolved(&store(), &[], &query).expect_err("this query cannot be served");
    };
    refused(GraphQuery::default());
    refused(GraphQuery {
        select: Selection {
            kind: Some(EntityKind::PERSON),
            ..Selection::default()
        },
        follow: Some(Follow {
            depth: 0,
            ..Follow::hop()
        }),
        ..GraphQuery::default()
    });
    refused(GraphQuery {
        select: Selection {
            kind: Some(EntityKind::PERSON),
            ..Selection::default()
        },
        follow: Some(Follow {
            depth: MAX_DEPTH + 1,
            ..Follow::hop()
        }),
        ..GraphQuery::default()
    });

    // The positive the three refusals rest on: the same walk one hop
    // shallower is served, so "refused" is about the argument and not
    // about walks.
    resolved(
        &store(),
        &[],
        &GraphQuery {
            select: Selection {
                kind: Some(EntityKind::PERSON),
                ..Selection::default()
            },
            follow: Some(Follow {
                depth: MAX_DEPTH,
                ..Follow::hop()
            }),
            ..GraphQuery::default()
        },
    )
    .expect("a walk within the ceiling is served");
}

/// **A walk narrows to the things that FIT a type, which is not the same
/// as the things that answer it.**
///
/// The bike's repair record carries `owner`, and `owner` is one of `pet`'s
/// keys — so the bike ANSWERS `pet`, partially, and a narrowing that
/// admitted partial answers would keep it. Worse, it could never exclude
/// anything: the walk travels `owner`, so everything it reaches answers the
/// type by construction. Fitting — every key the type names — is what makes
/// the narrowing mean something.
#[test]
fn a_walk_keeps_what_fits_a_type_and_not_what_merely_answers_it() {
    let reached = |fits: Option<types::DeclaredType>| {
        let found = resolved(
            &kennel(),
            &[pet()],
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId("person:bart".into())),
                    ..Selection::default()
                },
                include: Include {
                    facts: false,
                    prose: false,
                    stood_for: false,
                },
                follow: Some(Follow {
                    along: Along::Relation("owner".into()),
                    direction: Some(Direction::In),
                    fits_type: fits,
                    ..Follow::hop()
                }),
                history: None,
            },
        )
        .expect("a declared relation is followable");
        let mut handles: Vec<String> = found[0]
            .connected
            .iter()
            .map(|o| o.entity.id.to_string())
            .collect();
        handles.sort();
        (handles, found[0].unwalked)
    };

    // Unnarrowed, the walk reaches the bike, and that is the right answer
    // to what it was asked.
    let (everything, _) = reached(None);
    assert_eq!(
        everything,
        vec![
            "pet:santas-little-helper".to_string(),
            "pet:snowball".to_string(),
            "thing:red-bike".to_string(),
        ],
    );

    // Narrowed to what fits, the bike drops out — and the pets, which hold
    // every key, stay. The pair is the point: a negative on its own would
    // pass on a walk that reached nothing at all.
    let (fitting, unwalked) = reached(Some(pet()));
    assert_eq!(
        fitting,
        vec![
            "pet:santas-little-helper".to_string(),
            "pet:snowball".to_string(),
        ],
    );
    // …and the bike is not silently gone: the walk reached it and did not
    // keep it, which is an edge nobody followed.
    assert!(
        unwalked,
        "a thing the narrowing dropped is an unfollowed edge, not an absence"
    );
}

/// The type the relation cases declare: a pet, whose `owner` is a
/// reference and whose `born` is a date.
fn pet() -> types::DeclaredType {
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
fn kennel() -> Vec<DocScan> {
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

/// **A declared reference key IS a walkable link, both ways.**
///
/// One name and two directions: out of the pet, `owner` reaches the person;
/// in to the person, `owner` reaches the records naming them. Nobody
/// **A key declared to hold a LIST of references walks to every one of
/// them, both ways.**
///
/// A list cell holds its items separated by commas, and the declaration is
/// what says the cell is a list — the same declaration the write guard
/// screens each item through. Read as one handle, a two-item cell is a
/// handle nobody has: the outbound walk reaches nothing and the inbound
/// walk matches nothing, with no error and no flag on either.
///
/// **A one-item list is indistinguishable from an ordinary reference**, so
/// nothing shows until a second handle is recorded — which is the case a
/// list exists for (rule 217).
///
/// Both directions, because they read the cell in different ways and either
/// can be right while the other is wrong.
#[test]
fn a_list_of_references_walks_to_every_handle_in_it() {
    let declarations = vec![types::DeclaredType::new(
        "outing",
        vec![types::Field::listing(
            "came_along",
            types::ValueType::Reference,
        )],
    )];
    let scanned = vec![
        doc(entity("person:bart", "Bart"), "Bart's page.", Vec::new()),
        doc(
            entity("person:milhouse", "Milhouse"),
            "The other one.",
            Vec::new(),
        ),
        doc(
            entity("event:winter-fest", "Winter Fest"),
            "The outing's page.",
            vec![Fact {
                fields: [(
                    "came_along".to_string(),
                    "person:bart, person:milhouse".to_string(),
                )]
                .into_iter()
                .collect(),
                ..fact("event:winter-fest", "f1", "who came along")
            }],
        ),
    ];
    let walk = |from: &str, direction: Direction| {
        resolved(
            &scanned,
            &declarations,
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId(from.into())),
                    ..Selection::default()
                },
                include: Include {
                    facts: false,
                    prose: false,
                    stood_for: false,
                },
                follow: Some(Follow {
                    along: Along::Relation("came_along".into()),
                    direction: Some(direction),
                    ..Follow::hop()
                }),
                history: None,
            },
        )
        .expect("a declared relation is followable")
    };

    let out = walk("event:winter-fest", Direction::Out);
    assert_eq!(
        handles(&out[0].connected),
        vec!["person:bart", "person:milhouse"],
        "a list cell read as one handle reaches nobody: {out:?}",
    );

    let back = walk("person:milhouse", Direction::In);
    assert_eq!(
        handles(&back[0].connected),
        vec!["event:winter-fest"],
        "the second name in the cell is not the whole cell, so an equality on the cell \
             finds nothing: {back:?}",
    );
}

/// **An unscoped walk reaches every handle in a declared list of
/// references**, the same handles the relation walk above reaches by the key's
/// own name. Reading a list cell as one value reached nobody: the walk asked
/// whether the WHOLE cell was a handle, and `person:bart, person:milhouse` is
/// not one.
///
/// Both directions and both ends of the list, because the first item can be
/// reached while the rest are not. Paired with the relation walk on the same
/// corpus, which already reaches them: the unscoped walk is held to agree.
#[test]
fn an_unscoped_walk_reaches_every_handle_in_a_declared_list_of_references() {
    let declarations = vec![types::DeclaredType::new(
        "outing",
        vec![types::Field::listing(
            "came_along",
            types::ValueType::Reference,
        )],
    )];
    let scanned = vec![
        doc(entity("person:bart", "Bart"), "Bart's page.", Vec::new()),
        doc(
            entity("person:milhouse", "Milhouse"),
            "The other one.",
            Vec::new(),
        ),
        doc(
            entity("event:winter-fest", "Winter Fest"),
            "The outing's page.",
            vec![Fact {
                fields: [(
                    "came_along".to_string(),
                    "person:bart, person:milhouse".to_string(),
                )]
                .into_iter()
                .collect(),
                ..fact("event:winter-fest", "f1", "who came along")
            }],
        ),
    ];
    let walk = |from: &str, along: Along, direction: Direction| {
        let found = resolved(
            &scanned,
            &declarations,
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId(from.into())),
                    ..Selection::default()
                },
                include: Include {
                    facts: false,
                    prose: false,
                    stood_for: false,
                },
                follow: Some(Follow {
                    along,
                    direction: Some(direction),
                    ..Follow::hop()
                }),
                history: None,
            },
        )
        .expect("a subject with a walk");
        handles(&found[0].connected)
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<String>>()
    };

    // The relation walk is the positive the unscoped one is held to.
    assert_eq!(
        walk(
            "person:milhouse",
            Along::Relation("came_along".into()),
            Direction::In
        ),
        vec!["event:winter-fest".to_string()],
    );
    for (from, direction, expected) in [
        (
            "event:winter-fest",
            Direction::Out,
            vec!["person:bart", "person:milhouse"],
        ),
        ("person:bart", Direction::In, vec!["event:winter-fest"]),
        ("person:milhouse", Direction::In, vec!["event:winter-fest"]),
    ] {
        assert_eq!(
            walk(from, Along::AnyEdge, direction),
            expected.into_iter().map(str::to_string).collect::<Vec<_>>(),
            "the unscoped walk {direction:?} from {from} misses a handle in the list",
        );
    }
}

/// declares an inverse, because there is nothing to declare — the same key
/// walked the other way is the other question.
#[test]
fn a_declared_reference_key_is_a_walkable_link_both_ways() {
    let scanned = kennel();
    let declarations = vec![pet()];
    let walk = |from: &str, relation: &str, direction: Direction| {
        resolved(
            &scanned,
            &declarations,
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId(from.into())),
                    ..Selection::default()
                },
                include: Include {
                    facts: false,
                    prose: false,
                    stood_for: false,
                },
                follow: Some(Follow {
                    along: Along::Relation(relation.into()),
                    direction: Some(direction),
                    ..Follow::hop()
                }),
                history: None,
            },
        )
        .expect("a declared relation is followable")
    };

    let forward = walk("pet:santas-little-helper", "owner", Direction::Out);
    assert_eq!(
        handles(&forward[0].connected),
        vec!["person:bart"],
        "the key reaches what it points at: {forward:?}",
    );
    assert_eq!(
        forward[0].connected[0].via,
        Some(Via {
            link: Link::Relation("owner".into()),
            direction: Direction::Out,
            retracted: false,
        }),
        "and the reached object says it came along the relation, outbound",
    );

    let reverse = walk("person:bart", "owner", Direction::In);
    assert_eq!(
        handles(&reverse[0].connected),
        vec!["pet:santas-little-helper", "pet:snowball", "thing:red-bike"],
        "and the same key walked inbound reaches every record pointing here: {reverse:?}",
    );
    assert_eq!(
        reverse[0].connected[0].via.as_ref().map(|v| v.direction),
        Some(Direction::In),
        "which the walk went inbound to reach",
    );
}

/// **A relation name is walkable because a declaration says so.** The same
/// store, the same records, the same key — and with nothing declared, the
/// name reaches nothing and says so rather than answering empty.
///
/// This is the case that keeps a key's name from being walked unless a
/// declaration backs it. The value is still a link to an unscoped walk.
#[test]
fn nothing_is_a_relation_until_a_declaration_says_it_is() {
    let query = GraphQuery {
        select: Selection {
            subject: Some(EntityId("pet:santas-little-helper".into())),
            ..Selection::default()
        },
        include: Include {
            facts: false,
            prose: false,
            stood_for: false,
        },
        follow: Some(Follow {
            along: Along::Relation("owner".into()),
            ..Follow::hop()
        }),
        history: None,
    };
    resolved(&kennel(), &[], &query)
        .expect_err("with nothing declared, the key's name is no relation to walk by");

    // The same name, declared as ordinary text rather than a reference: the
    // value is identical and it is still not a link.
    let as_text = types::DeclaredType::new(
        "pet",
        vec![types::Field::required("owner", types::ValueType::Text)],
    );
    resolved(&kennel(), std::slice::from_ref(&as_text), &query)
        .expect_err("a key declared to hold text is not a relation");

    // …and the positive in the same case, so the two refusals are about the
    // declaration and not about relations.
    resolved(&kennel(), &[pet()], &query).expect("declared as a reference, it is one");
}

/// **One key name, two types, and the text one does not bury the
/// reference.** Two types may name one key and mean their own thing by it,
/// so a type calling `owner` text says nothing about the type calling it a
/// reference — the relation stays walkable either way.
///
/// **Both declaration orders, because the store hands them over sorted by
/// type name.** A lookup that stopped at the first declaration owning the
/// name would make walkability depend on alphabetical spelling, and a test
/// fixing one order would pass on it half the time.
#[test]
fn a_text_key_of_another_type_does_not_hide_a_declared_relation() {
    let shelf = types::DeclaredType::new(
        "book",
        vec![types::Field::required("owner", types::ValueType::Text)],
    );
    let walk = |declarations: &[types::DeclaredType]| {
        resolved(
            &kennel(),
            declarations,
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId("pet:santas-little-helper".into())),
                    ..Selection::default()
                },
                include: Include {
                    facts: false,
                    prose: false,
                    stood_for: false,
                },
                follow: Some(Follow {
                    along: Along::Relation("owner".into()),
                    direction: Some(Direction::Out),
                    ..Follow::hop()
                }),
                history: None,
            },
        )
        .expect("some type declares 'owner' a reference, so it is one")
    };

    for declarations in [vec![shelf.clone(), pet()], vec![pet(), shelf.clone()]] {
        let found = walk(&declarations);
        assert_eq!(
            handles(&found[0].connected),
            vec!["person:bart"],
            "the reference declaration is the one that counts, whichever came first: \
                 {found:?}",
        );
    }
}

/// **Two reference keys of one type stay apart, because they are two
/// KEYS.** A trip's `from` and its `to` both point at places, and each name
/// reaches its own end — inbound as well as outbound.
#[test]
fn two_reference_keys_of_one_type_do_not_collide() {
    let trip = types::DeclaredType::new(
        "trip",
        vec![
            types::Field::required("from", types::ValueType::Reference),
            types::Field::required("to", types::ValueType::Reference),
        ],
    );
    let travelling = Fact {
        fields: [
            ("from".to_string(), "place:springfield".to_string()),
            ("to".to_string(), "place:shelbyville".to_string()),
        ]
        .into_iter()
        .collect(),
        ..fact("person:bart", "f1", "went over for the day")
    };
    let scanned = vec![
        doc(entity("person:bart", "Bart"), "", vec![travelling]),
        doc(entity("place:springfield", "Springfield"), "", Vec::new()),
        doc(entity("place:shelbyville", "Shelbyville"), "", Vec::new()),
    ];

    let reached = |from: &str, relation: &str, direction: Direction| {
        let found = resolved(
            &scanned,
            std::slice::from_ref(&trip),
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId(from.into())),
                    ..Selection::default()
                },
                include: Include {
                    facts: false,
                    prose: false,
                    stood_for: false,
                },
                follow: Some(Follow {
                    along: Along::Relation(relation.into()),
                    direction: Some(direction),
                    ..Follow::hop()
                }),
                history: None,
            },
        )
        .expect("both keys are declared references");
        handles(&found[0].connected)
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
    };

    assert_eq!(
        reached("person:bart", "from", Direction::Out),
        vec!["place:springfield"],
    );
    assert_eq!(
        reached("person:bart", "to", Direction::Out),
        vec!["place:shelbyville"],
    );
    assert_eq!(
        reached("place:springfield", "from", Direction::In),
        vec!["person:bart"],
        "walked inbound, one key reaches the traveller",
    );
    assert!(
        reached("place:springfield", "to", Direction::In).is_empty(),
        "and the OTHER key does not — the two keys are what keeps the ends apart",
    );
}

/// **A relation is scoped by its KEY, and the direction chooses the way.**
///
/// The inbound walk reaches every record pointing here through that key,
/// whatever else each record is. The bike's repair record carries `owner`
/// and is reached — which is what the walk was asked for, and is why the
/// name claims nothing about pets.
///
/// **A reverse walk that also required the record to answer the declared
/// type, under a name like `pet.owner`, could not fail this case.** That
/// check excludes nothing: the walked key is one of the type's keys, and
/// holding one key is what answering a type means. So there is no such
/// check, and no name advertising one.
#[test]
fn a_relation_is_scoped_by_its_key_and_the_direction_chooses_the_way() {
    let scanned = kennel();
    let declarations = vec![pet()];
    let walk = |from: &str, relation: &str, direction: Direction| {
        resolved(
            &scanned,
            &declarations,
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId(from.into())),
                    ..Selection::default()
                },
                include: Include {
                    facts: false,
                    prose: false,
                    stood_for: false,
                },
                follow: Some(Follow {
                    along: Along::Relation(relation.into()),
                    direction: Some(direction),
                    ..Follow::hop()
                }),
                history: None,
            },
        )
    };

    let pointing_here =
        walk("person:bart", "owner", Direction::In).expect("a declared key is followable inbound");
    assert_eq!(
        handles(&pointing_here[0].connected),
        vec!["pet:santas-little-helper", "pet:snowball", "thing:red-bike"],
        "everything pointing here through `owner`, and the bike is no pet: {pointing_here:?}",
    );
    assert_eq!(
        pointing_here[0].connected[2].via,
        Some(Via {
            link: Link::Relation("owner".into()),
            direction: Direction::In,
            retracted: false,
        }),
        "and it says it arrived along the key, inbound",
    );

    // The same key the other way, off the record that is not a pet: one
    // name, and the direction is what picks the question.
    let points_at = walk("thing:red-bike", "owner", Direction::Out)
        .expect("a declared key is followable outbound");
    assert_eq!(handles(&points_at[0].connected), vec!["person:bart"]);

    // …and the object with no record at all is reached by neither, so the
    // hits above are about the key rather than about every object.
    assert!(
        walk("thing:floor-pump", "owner", Direction::Out).expect("served")[0]
            .connected
            .is_empty(),
    );

    // A qualified name is no relation, and the refusal offers the key
    // instead.
    let refused = walk("person:bart", "pet.owner", Direction::In)
        .expect_err("`type.key` is no relation name");
    match refused {
        MemoryError::InvalidQuery(why) => assert!(
            why.contains("owner"),
            "the refusal names the relation that does exist: {why}",
        ),
        other => panic!("a name no declaration backs is a malformed query: {other:?}"),
    }
}

/// **A key may be spelled like an edge shape, and the two do not cross.**
///
/// Bare key names make this reachable: nothing stops a type declaring a key
/// called `location`, and there is an edge shape of that name. They are
/// different vocabularies and `follow` says which one it means by which
/// argument it carries, so the same word reaches two different places.
///
/// Both halves in one case, because either alone passes on a build that
/// collapses the two into one lookup.
#[test]
fn a_key_named_like_an_edge_shape_is_still_a_key() {
    let declared = types::DeclaredType::new(
        "posting",
        vec![types::Field::required(
            "location",
            types::ValueType::Reference,
        )],
    );
    let by_key = Fact {
        fields: [("location".to_string(), "place:shelbyville".to_string())]
            .into_iter()
            .collect(),
        ..fact("person:bart", "f1", "posted from over there")
    };
    let by_edge = edged(
        "person:bart",
        "f2",
        "actually lives here",
        EdgeShape::Location,
        "place:springfield",
    );
    let scanned = vec![
        doc(entity("person:bart", "Bart"), "", vec![by_key, by_edge]),
        doc(entity("place:springfield", "Springfield"), "", Vec::new()),
        doc(entity("place:shelbyville", "Shelbyville"), "", Vec::new()),
    ];

    let along = |along: Along| {
        let found = resolved(
            &scanned,
            std::slice::from_ref(&declared),
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId("person:bart".into())),
                    ..Selection::default()
                },
                include: Include {
                    facts: false,
                    prose: false,
                    stood_for: false,
                },
                follow: Some(Follow {
                    along,
                    ..Follow::hop()
                }),
                history: None,
            },
        )
        .expect("both vocabularies are followable");
        handles(&found[0].connected)
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
    };

    assert_eq!(
        along(Along::Relation("location".into())),
        vec!["place:shelbyville"],
        "the KEY reaches what the record's value names",
    );
    assert_eq!(
        along(Along::Edge(EdgeShape::Location)),
        vec!["place:springfield"],
        "and the SHAPE reaches what the edge points at — same word, two vocabularies",
    );
}

/// **The declared value type licenses the operator.** A key declared to
/// hold a date can be asked what is before a date; the same key with
/// nothing declared has equality and the ordering is refused.
///
/// Both halves matter: refusing is what stops a caller reading an equality
/// answer as an ordering one.
#[test]
fn a_declaration_licenses_an_ordering_and_nothing_else_does() {
    let scanned = kennel();
    let born_before = |declarations: &[types::DeclaredType]| {
        resolved(
            &scanned,
            declarations,
            &GraphQuery {
                select: Selection {
                    fields: vec![FieldFilter::comparing(
                        "born",
                        types::Compare::Before,
                        "2020-01-01",
                    )],
                    ..Selection::default()
                },
                ..GraphQuery::default()
            },
        )
    };
    let found = born_before(&[pet()]).expect("the declaration licenses it");
    assert_eq!(
        handles(&found),
        vec!["pet:santas-little-helper"],
        "the older pet is before the date and the younger one is not: {found:?}",
    );

    born_before(&[]).expect_err("with nothing declared, an ordering is refused");

    // The number half of the same rule, and its own negative: `less` is
    // licensed by a declared number, and asking it of the date key is not.
    let lighter = resolved(
        &scanned,
        &[pet()],
        &GraphQuery {
            select: Selection {
                fields: vec![FieldFilter::comparing("weight", types::Compare::Less, "10")],
                ..Selection::default()
            },
            ..GraphQuery::default()
        },
    )
    .expect("a declared number licenses an ordering");
    assert_eq!(handles(&lighter), vec!["pet:snowball"]);

    resolved(
        &scanned,
        &[pet()],
        &GraphQuery {
            select: Selection {
                fields: vec![FieldFilter::comparing("born", types::Compare::Less, "10")],
                ..Selection::default()
            },
            ..GraphQuery::default()
        },
    )
    .expect_err("a date key does not license a number's ordering");
}

/// **An undeclared record keeps equality.** Matching stays structural: a
/// record is found by the keys it carries whether or not anybody declared a
/// type for it, and nothing about the ordering above narrows that.
#[test]
fn an_undeclared_record_keeps_equality() {
    let found = resolved(
        &kennel(),
        &[],
        &GraphQuery {
            select: Selection {
                fields: vec![FieldFilter::holding("born", "2024-11-02")],
                ..Selection::default()
            },
            ..GraphQuery::default()
        },
    )
    .expect("equality needs no declaration");
    assert_eq!(handles(&found), vec!["pet:snowball"]);
}

/// **A walk can filter what it reaches**, which is what the record filters
/// could not do: they describe the roots.
///
/// The acceptance case, end to end: from the owner, walk the has-many, and
/// keep only the pets born before a date. Paired with the same walk
/// unfiltered, so the filter is doing the work rather than the graph being
/// that shape anyway.
#[test]
fn a_walk_keeps_only_what_answers_its_filters() {
    let scanned = kennel();
    let declarations = vec![pet()];
    let from_bart = |keeping: Vec<FieldFilter>| {
        resolved(
            &scanned,
            &declarations,
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId("person:bart".into())),
                    ..Selection::default()
                },
                include: Include::default(),
                follow: Some(Follow {
                    along: Along::Relation("owner".into()),
                    direction: Some(Direction::In),
                    keeping,
                    ..Follow::hop()
                }),
                history: None,
            },
        )
        .expect("a walk with filters on what it reaches")
    };

    let all = from_bart(Vec::new());
    assert_eq!(
        handles(&all[0].connected),
        vec!["pet:santas-little-helper", "pet:snowball", "thing:red-bike"],
        "unfiltered, the walk reaches everything pointing here through the key: {all:?}",
    );

    let older = from_bart(vec![FieldFilter::comparing(
        "born",
        types::Compare::Before,
        "2020-01-01",
    )]);
    assert_eq!(
        handles(&older[0].connected),
        vec!["pet:santas-little-helper"],
        "and filtered, it reaches only the pet born before the date: {older:?}",
    );
    assert!(
        older[0].unwalked,
        "the pet it did not keep is an edge nobody followed, and the object says so: {:?}",
        older[0],
    );
    assert!(
        older[0].connected[0]
            .facts
            .iter()
            .all(|f| f.content == "the greyhound"),
        "a kept object arrives with the records that answered: {:?}",
        older[0].connected[0],
    );
}

/// **A walk's filters are asked the same two ways a selection's are**, and
/// which way decides both what the walk reaches and what each object brings.
///
/// The greyhound is described over two records: one carries `born` and one
/// does not. Asked of the THING, `born` is answered by the fold and the
/// object arrives whole. Asked of a RECORD, it is answered by the one row
/// carrying the key, and only that row rides along. Both in one case,
/// because either alone passes on a build where every filter is the other
/// scope.
#[test]
fn a_walks_filter_is_asked_of_the_thing_or_of_one_record() {
    let mut scanned = kennel();
    let greyhound = scanned
        .iter_mut()
        .find(|d| d.doc_id == "pet:santas-little-helper")
        .expect("the kennel holds the greyhound");
    greyhound
        .facts
        .push(fact("pet:santas-little-helper", "f2", "went to the vet"));
    let declarations = vec![pet()];
    let from_bart = |keeping: Vec<FieldFilter>| {
        resolved(
            &scanned,
            &declarations,
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId("person:bart".into())),
                    ..Selection::default()
                },
                follow: Some(Follow {
                    along: Along::Relation("owner".into()),
                    direction: Some(Direction::In),
                    keeping,
                    ..Follow::hop()
                }),
                ..GraphQuery::default()
            },
        )
        .expect("a walk with filters on what it reaches")
    };

    let of_the_thing = from_bart(vec![FieldFilter::comparing(
        "born",
        types::Compare::Before,
        "2020-01-01",
    )]);
    assert_eq!(
        handles(&of_the_thing[0].connected),
        vec!["pet:santas-little-helper"],
        "the fold answers `born`, so the walk reaches the greyhound: {of_the_thing:?}",
    );
    assert_eq!(
        of_the_thing[0].connected[0].facts.len(),
        2,
        "and a filter about the thing does not narrow the thing's page: {:?}",
        of_the_thing[0].connected[0],
    );

    let of_a_record = from_bart(vec![
        FieldFilter::comparing("born", types::Compare::Before, "2020-01-01").on_a_record(),
    ]);
    assert_eq!(
        handles(&of_a_record[0].connected),
        vec!["pet:santas-little-helper"],
        "the same object, reached because one record of its answers: {of_a_record:?}",
    );
    assert_eq!(
        of_a_record[0].connected[0]
            .facts
            .iter()
            .map(|f| f.content.as_str())
            .collect::<Vec<_>>(),
        vec!["the greyhound"],
        "and it brings the record that answered, not the vet trip: {:?}",
        of_a_record[0].connected[0],
    );
}

/// **A fact homed on one page and about another belongs to both.** The
/// rule `recall` already answers by, kept here so one record does not
/// belong to an entity through one verb and not the other.
#[test]
fn a_fact_belongs_to_its_subject_and_to_the_page_it_sits_on() {
    let mut scanned = store();
    let visiting = Fact {
        subject: EntityId("person:ralph".into()),
        ..fact("event:birthday-party", "f2", "brings the pudding")
    };
    scanned[0].facts.push(visiting);

    let says = |handle: &str| {
        resolved(&scanned, &[], &GraphQuery::subject(EntityId(handle.into())))
            .expect("a subject")
            .swap_remove(0)
            .facts
            .iter()
            .any(|f| f.content == "brings the pudding")
    };
    assert!(says("person:ralph"), "it is about her");
    assert!(says("event:birthday-party"), "and it sits on the party");
}

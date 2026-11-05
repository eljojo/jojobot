//! **Tables for six rules of `memory.rs`, asked of the rule alone.** Each rule
//! has been proven only by a contract case that runs it through a store; these
//! tables ask the deciding function directly. They pin the variant a refusal
//! takes and the clause that fires, never the refusal's prose.

use super::*;
use jiff::civil::date;

fn who(handle: &str) -> EntityId {
    EntityId(handle.into())
}

fn day(offset: i8) -> Date {
    date(2026, 3, 10 + offset)
}

fn claim(content: &str) -> NewFact {
    NewFact::about(who("person:milhouse"), content, day(0))
}

/// A stored claim to retract: the shape the stores hand `retract`.
fn stored(id: &str, status: FactStatus, fields: &[(&str, &str)]) -> Fact {
    let milhouse = who("person:milhouse");
    Fact {
        app_stamp: None,
        written_by_another: false,
        sets: Default::default(),
        reaching: Default::default(),
        field_links: Default::default(),
        id: FactId(id.into()),
        home: milhouse.clone(),
        subject: milhouse,
        content: "keeps the ledger".into(),
        details: None,
        provenance: Provenance::Testimony,
        standing: Standing::Settled,
        status,
        recorded_at: day(0),
        happened_at: None,
        happened_through: None,
        edge: None,
        fields: fields
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect(),
        refs: Vec::new(),
        derived_from: None,
        stands_for: Vec::new(),
        inserted_at: None,
        stale_after: None,
        receipt: None,
    }
}

fn spanning(start: Option<Date>, end: Option<Date>) -> Fact {
    let mut fact = stored("f1", FactStatus::Active, &[]);
    fact.happened_at = start;
    fact.happened_through = end;
    fact
}

/// **A span covers every day from its start through its end; a single day
/// covers only itself; a claim that says nothing about when covers nothing.**
/// Both ends are inside the span, the day beside each end is outside it, and an
/// end with no start (a shape validation refuses, but a stored claim may hold)
/// covers nothing.
#[test]
fn a_claims_span_covers_exactly_the_days_it_names() {
    //   (start, end, [(day, covered)])
    type Span = (Option<Date>, Option<Date>, Vec<(Date, bool)>);
    let table: [Span; 5] = [
        (
            Some(day(0)),
            Some(day(3)),
            vec![
                (day(-1), false),
                (day(0), true),
                (day(2), true),
                (day(3), true),
                (day(4), false),
            ],
        ),
        (
            Some(day(2)),
            None,
            vec![(day(1), false), (day(2), true), (day(3), false)],
        ),
        (
            Some(day(2)),
            Some(day(2)),
            vec![(day(1), false), (day(2), true), (day(3), false)],
        ),
        (None, None, vec![(day(0), false)]),
        (None, Some(day(3)), vec![(day(0), false), (day(3), false)]),
    ];
    for (start, end, days) in table {
        let fact = spanning(start, end);
        for (asked, covered) in days {
            assert_eq!(
                fact.happened_covers(asked),
                covered,
                "span {start:?}..{end:?} asked of {asked}"
            );
        }
    }
}

/// **Only the key a retraction writes is refused when cleared.** Every other
/// key, however alike, is the caller's to clear; the check is on the trimmed
/// key, and one reserved key among others is enough.
#[test]
fn only_the_reserved_keys_are_refused_when_cleared() {
    let keys = |list: &[&str]| list.iter().map(|k| k.to_string()).collect::<Vec<_>>();
    //   (cleared keys, refused)
    let table = [
        (keys(&[]), false),
        (keys(&["status"]), false),
        (keys(&["retracts_not"]), false),
        (keys(&[RETRACTS]), true),
        (keys(&["  retracts  "]), true),
        (keys(&["status", MERGED_FROM]), true),
        (keys(&[MERGED_FROM, "status"]), true),
    ];
    for (cleared, refused) in table {
        let outcome = validate_cleared_fields(&cleared);
        assert_eq!(
            matches!(outcome, Err(MemoryError::InvalidFact(_))),
            refused,
            "{cleared:?}: {outcome:?}"
        );
        assert!(outcome.is_ok() || refused, "{cleared:?}: {outcome:?}");
    }
}

/// **What can be taken back.** An archived row is already so (its own
/// variant, not a second sentence of one refusal); a retraction cannot be
/// retracted; anything else active can. Archived wins over retraction when a
/// row is both, because the state is the first fact asked.
#[test]
fn a_row_can_be_retracted_once_and_a_retraction_never() {
    enum Verdict {
        Retractable,
        Already,
        Not,
    }
    let marker = (RETRACTS, "person:milhouse#f9");
    //   (row, verdict)
    let table = [
        (stored("f1", FactStatus::Active, &[]), Verdict::Retractable),
        (
            stored("f2", FactStatus::Active, &[("note", "x")]),
            Verdict::Retractable,
        ),
        (stored("f3", FactStatus::Archived, &[]), Verdict::Already),
        (stored("f4", FactStatus::Active, &[marker]), Verdict::Not),
        (
            stored("f5", FactStatus::Archived, &[marker]),
            Verdict::Already,
        ),
    ];
    for (row, verdict) in table {
        let outcome = check_retractable(&row);
        match (verdict, &outcome) {
            (Verdict::Retractable, Ok(())) => {}
            (Verdict::Already, Err(MemoryError::AlreadyRetracted { attempted }))
            | (Verdict::Not, Err(MemoryError::NotRetractable { attempted, .. })) => {
                assert_eq!(attempted, &row.address().to_string());
            }
            _ => panic!("{row:?}: {outcome:?}"),
        }
    }
}

/// **The account a retraction leaves.** It is homed with the row it takes
/// back, names it by address in the reserved key and nothing else in the bag,
/// carries the reason when one was given (trimmed, a blank one is none) and a
/// stated absence when none was; a multi-line reason is refused; a row that
/// cannot be retracted yields no account.
#[test]
fn a_retraction_account_names_its_target_and_carries_the_reason_or_its_absence() {
    let target = stored("f1", FactStatus::Active, &[]);
    let with_reason = retraction_of(&target, Some("  it was a mistake  "), day(1)).expect("builds");
    assert_eq!(with_reason.subject, target.subject);
    assert_eq!(with_reason.content, normalize_content("it was a mistake"));
    assert_eq!(with_reason.recorded_at, day(1));
    assert_eq!(
        with_reason.fields,
        BTreeMap::from([(RETRACTS.to_string(), target.address().to_string())]),
        "the link is the only field"
    );
    let reasons_that_are_none = [None, Some(""), Some("   ")];
    let absent: Vec<String> = reasons_that_are_none
        .into_iter()
        .map(|reason| {
            retraction_of(&target, reason, day(1))
                .expect("builds")
                .content
        })
        .collect();
    assert!(
        absent.iter().all(|content| content == &absent[0]),
        "no reason, a blank reason and a space are the same account: {absent:?}"
    );
    assert_ne!(absent[0], with_reason.content);
    // **Every account has content**: a row needs a sentence even when nobody said
    // why, so the account of an absent reason is one the content rule accepts.
    // Equal to each other and different from the reasoned one is satisfied by an
    // empty string, which the store would refuse to write.
    for content in &absent {
        assert_eq!(validate_content(content), Ok(()), "{content:?}");
    }
    assert!(matches!(
        retraction_of(&target, Some("one\ntwo"), day(1)),
        Err(MemoryError::InvalidFact(_))
    ));
    let archived = stored("f2", FactStatus::Archived, &[]);
    assert!(matches!(
        retraction_of(&archived, Some("why"), day(1)),
        Err(MemoryError::AlreadyRetracted { .. })
    ));
    let retraction = stored(
        "f3",
        FactStatus::Active,
        &[(RETRACTS, "person:milhouse#f1")],
    );
    assert!(matches!(
        retraction_of(&retraction, None, day(1)),
        Err(MemoryError::NotRetractable { .. })
    ));
}

/// **The checks a new claim faces, in the order they are made.** One fault at
/// a time names its own variant; two faults name the earlier. Records written
/// beside the claim are refused first, then the subject, the content, the edge,
/// the span, the fields, and last the source an observation owes.
#[test]
fn a_new_claim_is_refused_by_the_first_clause_it_breaks() {
    kinds::load_shipped();
    let mut beside = claim("keeps the ledger");
    beside.also_written = vec![claim("another")];
    let session = NewFact::about(who("session:alpha"), "keeps the ledger", day(0));
    let blank = claim("   ");
    let two_lines = claim("one\ntwo");
    let mut bad_edge = claim("keeps the ledger");
    bad_edge.edge = Some(Edge::new(EdgeShape::Location, who("person:homer")));
    let mut orphan_end = claim("keeps the ledger");
    orphan_end.happened_through = Some(day(2));
    let mut reserved = claim("keeps the ledger");
    reserved.fields.insert(RETRACTS.into(), "x".into());
    let mut unsourced = claim("keeps the ledger");
    unsourced.provenance = Provenance::Observation;
    let mut sourced = claim("keeps the ledger");
    sourced.provenance = Provenance::Observation;
    sourced.fields.insert(READ_FROM.into(), "the ledger".into());
    let mut blank_source = claim("keeps the ledger");
    blank_source.provenance = Provenance::Observation;
    blank_source.fields.insert(READ_FROM.into(), "  ".into());
    // Two faults of different variants: the earlier clause names the refusal.
    let mut subject_then_content = NewFact::about(who("session:alpha"), "   ", day(0));
    subject_then_content.provenance = Provenance::Observation;
    let mut content_then_edge = claim("   ");
    content_then_edge.edge = Some(Edge::new(EdgeShape::Location, who("person:homer")));
    let mut edge_then_source = claim("keeps the ledger");
    edge_then_source.edge = Some(Edge::new(EdgeShape::Location, who("person:homer")));
    edge_then_source.provenance = Provenance::Observation;
    let mut span_then_fields = claim("keeps the ledger");
    span_then_fields.happened_through = Some(day(2));
    span_then_fields
        .fields
        .insert(RETRACTS.into(), "person:milhouse#f1".into());
    let mut beside_then_content = claim("   ");
    beside_then_content.also_written = vec![claim("another")];
    // The faults alone, to tell which clause decided a refusal of one variant.
    let (span_alone, fields_alone) = (orphan_end.clone(), reserved.clone());
    let (beside_alone, content_alone) = (beside.clone(), blank.clone());
    let (two_span_fields, two_beside_content) =
        (span_then_fields.clone(), beside_then_content.clone());

    #[derive(Debug)]
    enum Refused {
        Fact,
        Subject,
        Edge,
        Unsourced,
        Nothing,
    }
    //   (claim, what refuses it)
    let table = [
        (claim("keeps the ledger"), Refused::Nothing),
        (sourced, Refused::Nothing),
        (beside, Refused::Fact),
        (session, Refused::Subject),
        (blank, Refused::Fact),
        (two_lines, Refused::Fact),
        (bad_edge, Refused::Edge),
        (orphan_end, Refused::Fact),
        (reserved, Refused::Fact),
        (unsourced, Refused::Unsourced),
        (blank_source, Refused::Unsourced),
        (subject_then_content, Refused::Subject),
        (content_then_edge, Refused::Fact),
        (edge_then_source, Refused::Edge),
        (span_then_fields, Refused::Fact),
    ];
    for (fact, refused) in table {
        let outcome = validate_new_fact(&fact);
        let held = matches!(
            (&refused, &outcome),
            (Refused::Nothing, Ok(()))
                | (Refused::Fact, Err(MemoryError::InvalidFact(_)))
                | (Refused::Subject, Err(MemoryError::InvalidSubject(_)))
                | (Refused::Edge, Err(MemoryError::InvalidEdge(_)))
                | (Refused::Unsourced, Err(MemoryError::UnsourcedObservation))
        );
        assert!(held, "{fact:?} expected {refused:?}, got {outcome:?}");
    }
    // **Faults of one variant are told apart by the refusal each makes alone**:
    // two faults must come back as the earlier clause's refusal, and that
    // refusal must differ from the later clause's, or the pair proves nothing.
    for (both, earlier, later) in [
        (two_span_fields, span_alone, fields_alone),
        (two_beside_content, beside_alone, content_alone),
    ] {
        let (first, second) = (validate_new_fact(&earlier), validate_new_fact(&later));
        assert!(first.is_err() && second.is_err() && first != second);
        assert_eq!(validate_new_fact(&both), first, "{both:?}");
    }
    let week = NewFact::about(who("usage:alpha"), "keeps the ledger", day(0));
    assert!(matches!(
        validate_new_fact(&week),
        Err(MemoryError::WeekOfUsageWritten(_))
    ));
}

/// **The shape of a new entity.** Each clause names the same variant, so the
/// table walks the inputs one fault at a time, plus the two kinds that must
/// name a parent and the rule that a parent must be a handle.
#[test]
fn a_new_entity_is_refused_by_the_clause_it_breaks() {
    kinds::load_shipped();
    let ok = |id: &str, name: &str| validate_entity(&who(id), name, &[], "the fixture", None, None);
    let long = "a".repeat(201);
    let edge = "a".repeat(200);
    assert!(ok("person:milhouse", "Milhouse").is_ok());
    assert!(ok("person:milhouse", &edge).is_ok(), "200 is the limit");
    //   (outcome, refused as an entity)
    let table = [
        (ok("person:milhouse", ""), true),
        (ok("person:milhouse", "   "), true),
        (ok("person:milhouse", &long), true),
        (ok("person:milhouse", "two\nlines"), true),
        (ok("person:milhouse", "back`tick"), true),
        (ok("person:milhouse/2", "Milhouse"), true),
        (
            validate_entity(
                &who("person:milhouse"),
                "M",
                &["a,b".into()],
                "src",
                None,
                None,
            ),
            true,
        ),
        (
            validate_entity(
                &who("person:milhouse"),
                "M",
                &["".into()],
                "src",
                None,
                None,
            ),
            true,
        ),
        (
            validate_entity(&who("person:milhouse"), "M", &[], "", None, None),
            true,
        ),
        (
            validate_entity(
                &who("person:milhouse"),
                "M",
                &[],
                "src",
                Some("card 4"),
                None,
            ),
            true,
        ),
        (
            validate_entity(&who("person:milhouse"), "M", &[], "src", Some("a,b"), None),
            true,
        ),
        (
            validate_entity(
                &who("person:milhouse"),
                "M",
                &[],
                "src",
                Some("card:4"),
                None,
            ),
            false,
        ),
        (
            validate_entity(&who("rhythm:alpha"), "Loop", &[], "src", None, None),
            true,
        ),
        (
            validate_entity(&who("promise:alpha"), "Promise", &[], "src", None, None),
            true,
        ),
        (
            validate_entity(
                &who("rhythm:alpha"),
                "Loop",
                &[],
                "src",
                None,
                Some(&who("thing:beta")),
            ),
            false,
        ),
        (
            validate_entity(
                &who("promise:alpha"),
                "Promise",
                &[],
                "src",
                None,
                Some(&who("person:homer")),
            ),
            false,
        ),
    ];
    for (outcome, refused) in table {
        match (refused, &outcome) {
            (true, Err(MemoryError::InvalidEntity(_))) | (false, Ok(())) => {}
            _ => panic!("expected refused={refused}, got {outcome:?}"),
        }
    }
    // A parent that is not a handle is refused as a subject, not as an entity:
    // the guard answers the other parent questions.
    let malformed = validate_entity(
        &who("thing:alpha"),
        "Alpha",
        &[],
        "src",
        None,
        Some(&who("not a handle")),
    );
    assert!(
        matches!(malformed, Err(MemoryError::InvalidSubject(_))),
        "{malformed:?}"
    );
    // A session is not an entity to create.
    assert!(matches!(
        ok("session:alpha", "Alpha"),
        Err(MemoryError::InvalidSubject(_))
    ));
}

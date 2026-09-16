use super::support::{add, capture, edit, ensure};
use super::*;

/// Add an entity the guard is expected to **refuse first** — the way a
/// caller really gets one made: read the refusal, take the token it minted,
/// come again with it.
///
/// The token is read out of the answer rather than made up beside it. A
/// setup line that mints its own token proves nothing about the store it is
/// setting up, and would pass on one that accepts any string.
async fn add_over_the_screen<M: Memory>(store: &M, new: NewEntity) -> Entity {
    let id = new.id.clone();
    let Guarded::Blocked {
        attempted,
        candidates,
    } = store
        .add_entity(new.clone())
        .await
        .expect("add_entity should succeed")
    else {
        panic!("{id} was expected to hit the near-miss screen and did not");
    };
    store
        .add_entity(NewEntity {
            override_token: Some(guard::override_token(&attempted, &candidates)),
            ..new
        })
        .await
        .expect("add_entity should succeed")
        .written()
        .unwrap_or_else(|| panic!("the refusal's own token must let {id} through"))
}

/// Fetch the fact the store returned from `capture`, read back by id.
async fn read_back<M: Memory>(store: &M, subject: &EntityId, id: &FactId) -> Fact {
    store
        .recall(subject)
        .await
        .expect("recall should succeed")
        .into_iter()
        .find(|f| &f.id == id)
        .unwrap_or_else(|| panic!("recall must return the captured fact (id {id})"))
}

/// **The claim an address names, or nothing** — followed the ordinary way,
/// through a recall of the doc the address points into.
///
/// This is what "a pointer resolves" means to a caller: it has an address
/// and nothing else, so it goes to the home and looks for the id. **It
/// re-computes no part of the answer** — a case that worked out for itself
/// where a claim ought to have ended up would stay green over a store that
/// put it somewhere else.
async fn claim_at<M: Memory>(store: &M, address: &FactAddress) -> Option<Fact> {
    store
        .recall(&address.home)
        .await
        .ok()
        .and_then(|facts| facts.into_iter().find(|f| f.id == address.local))
}

/// **What the thing IS**, read the way the served answer reads it: one
/// dense row, folded, off the store rather than off records the caller
/// folded for itself.
async fn thing_fields<M: Memory>(
    store: &M,
    id: &EntityId,
) -> std::collections::BTreeMap<String, String> {
    graph::walk(
        store,
        &graph::GraphQuery {
            select: graph::Selection {
                subject: Some(id.clone()),
                ..graph::Selection::default()
            },
            include: graph::Include {
                facts: false,
                prose: false,
                stood_for: false,
            },
            follow: None,
            history: None,
        },
    )
    .await
    .expect("a handle is a selection")
    .objects
    .first()
    .unwrap_or_else(|| panic!("a walk from {id} must answer with the thing itself"))
    .fields
    .clone()
}

/// One entity out of the store's own listing — the read path for entities.
async fn read_entity<M: Memory>(store: &M, id: &EntityId) -> Entity {
    store
        .list_entities(None)
        .await
        .expect("list_entities should succeed")
        .into_iter()
        .find(|e| &e.id == id)
        .unwrap_or_else(|| panic!("list_entities must return {id}"))
}

// --- facts (slice 1, still binding) --------------------------------------

/// The core invariant: a captured fact is returned by a later recall.
pub async fn capture_reads_back<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-readback");
    let captured = capture(
        store,
        NewFact::about(subject.clone(), "drinks oat milk", date(2026, 7, 24)),
    )
    .await;
    let seen = read_back(store, &subject, &captured.id).await;
    assert_eq!(seen, captured, "recalled fact must be byte-identical");
}

/// **A `derived_from` names a claim, and the claim has to be there.**
///
/// The surface teaches that everything a write names must already exist,
/// and a claim is named as surely as an entity is. A field that takes any
/// well-formed address without looking makes a link to a claim that never
/// existed — a citation to nothing, and the reader it fails is a later
/// session following the provenance chain, which is the whole reason the
/// field is there.
///
/// Both halves, in one read. The refusal alone passes on a store whose
/// capture is simply broken; the acceptance alone passes on the store that
/// checked nothing.
pub async fn derived_from_must_name_a_fact_that_exists<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-derived");
    let source = capture(
        store,
        NewFact::about(subject.clone(), "said the ferry moved", date(2026, 4, 1)),
    )
    .await;

    // Named and there: accepted, and the link reads back off the record.
    let derived = capture(
        store,
        NewFact {
            derived_from: Some(source.address()),
            ..NewFact::about(
                subject.clone(),
                "so the crossing is longer now",
                date(2026, 4, 2),
            )
        },
    )
    .await;
    assert_eq!(
        read_back(store, &subject, &derived.id).await.derived_from,
        Some(source.address()),
        "a link to a claim that exists survives the write"
    );

    // Named and absent: refused, with the addresses that do exist.
    let missing = FactAddress::parse("person:contract-derived#f99").expect("well-formed");
    let refused = store
        .capture(NewFact {
            derived_from: Some(missing.clone()),
            ..NewFact::about(subject.clone(), "and the fare went up", date(2026, 4, 3))
        })
        .await;
    let Err(MemoryError::UnknownFact { attempted, nearest }) = &refused else {
        panic!("a derived_from naming no claim must be refused as a miss, got {refused:?}");
    };
    assert_eq!(attempted, &missing.to_string());
    assert!(
        nearest.contains(&source.address().to_string()),
        "the addresses that DO exist are what makes it repairable: {nearest:?}"
    );

    // A home nobody has heard of is an ENTITY miss, which is the shape
    // `retract` already answers with. Two shapes, no third: what is absent
    // differs, so what the caller does about it differs.
    let nowhere = FactAddress::parse("person:contract-nobody#f1").expect("well-formed");
    let refused = store
        .capture(NewFact {
            derived_from: Some(nowhere),
            ..NewFact::about(subject.clone(), "and the pier closed", date(2026, 4, 4))
        })
        .await;
    assert!(
        matches!(refused, Err(MemoryError::UnknownEntity { .. })),
        "a derived_from whose home is unknown is an entity miss, got {refused:?}"
    );

    // Nothing was written by either refusal. A guard that refuses and
    // writes anyway is the failure this whole rule exists to prevent
    // (rule 18).
    let facts = store.recall(&subject).await.expect("recall should succeed");
    assert_eq!(
        facts.len(),
        2,
        "a refused capture leaves the record as it was: {facts:?}"
    );
}

/// **The same rule, reached by an EDIT.** The case above proves it at
/// capture; `update_fact`'s own check of `derived_from` is the sibling that
/// was never driven through this path, and it disagrees with itself in both
/// directions there: a near miss on the fact answers with no candidates at
/// all (screened against the raw handle rather than the storage key it
/// resolves to), and a totally unknown home is answered as a FACT miss
/// instead of an ENTITY miss — the one shape `stands_for`'s own edit-path
/// check, right beside this one, already gets right.
///
/// Both halves, in one read, exactly as the capture case above: the refusal
/// alone passes on an edit path that is simply broken; the candidates alone
/// pass on a store that refuses but reports nothing repairable.
pub async fn derived_from_on_an_edit_must_name_a_fact_that_exists<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-citation-editpath");
    let source = capture(
        store,
        NewFact::about(subject.clone(), "said the ferry moved", date(2026, 4, 1)),
    )
    .await;
    let target = capture(
        store,
        NewFact::about(
            subject.clone(),
            "so the crossing is longer now",
            date(2026, 4, 2),
        ),
    )
    .await;

    // Named and there: accepted, and the link reads back off the record.
    let edited = edit(
        store,
        &target.address(),
        FactPatch {
            derived_from: Some(source.address()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        edited.derived_from,
        Some(source.address()),
        "a link to a claim that exists survives the edit"
    );

    // Named and absent, on a home that IS known: refused, with the
    // addresses that do exist.
    let missing = FactAddress::parse("person:contract-citation-editpath#f99").expect("well-formed");
    let refused = store
        .update_fact(
            &target.address(),
            FactPatch {
                derived_from: Some(missing.clone()),
                ..Default::default()
            },
        )
        .await;
    let Err(MemoryError::UnknownFact { attempted, nearest }) = &refused else {
        panic!("an edit's derived_from naming no claim must be refused as a miss, got {refused:?}");
    };
    assert_eq!(attempted, &missing.to_string());
    assert!(
        nearest.contains(&source.address().to_string()),
        "the addresses that DO exist are what makes it repairable: {nearest:?}"
    );

    // A home nobody has heard of is an ENTITY miss.
    let nowhere = FactAddress::parse("person:contract-orphaned-editpath#f1").expect("well-formed");
    let refused = store
        .update_fact(
            &target.address(),
            FactPatch {
                derived_from: Some(nowhere),
                ..Default::default()
            },
        )
        .await;
    assert!(
        matches!(refused, Err(MemoryError::UnknownEntity { .. })),
        "an edit's derived_from whose home is unknown is an entity miss, got {refused:?}"
    );

    // Nothing was written by either refusal.
    let after = read_back(store, &subject, &target.id).await;
    assert_eq!(
        after.derived_from,
        Some(source.address()),
        "a refused edit leaves the record exactly as the accepted one left it",
    );
}

/// Every field survives capture→recall unchanged and byte-identical —
/// `derived_from` included, since it is a fact field like any other and
/// this is the one test that pins ALL of them at once.
/// 🚨 **Told "over the summer", a claim records NO day, and that is the
/// whole point.**
///
/// One date column meant one slot and two meanings, so a writer with a
/// vague answer had to pick a day or lose the claim — and picking one puts
/// a date nobody stated on a record that may carry the operator's own word.
/// **Absent is a complete answer.**
///
/// ⛔️ **PAIRED, and the pair is what makes it a split rather than a
/// dropped column:** a claim told an actual day still records it, and the
/// claim's own date is untouched in both. **Without the second half this
/// passes against a store that silently discards every event date.**
pub async fn a_claim_can_say_nothing_about_when_the_thing_happened<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-summertime");
    ensure(store, &subject).await;

    let vague = capture(
        store,
        NewFact::about(subject.clone(), "brought the pump back", date(2026, 10, 11)),
    )
    .await;
    assert_eq!(
        vague.happened_at, None,
        "a claim nobody gave a day for invented one",
    );
    assert_eq!(
        vague.recorded_at,
        date(2026, 10, 11),
        "the claim's own date moved when the other one was left off",
    );

    let dated = capture(
        store,
        NewFact {
            happened_at: Some(date(2026, 6, 14)),
            ..NewFact::about(subject.clone(), "came to the survey", date(2026, 10, 11))
        },
    )
    .await;
    assert_eq!(
        dated.happened_at,
        Some(date(2026, 6, 14)),
        "a day the caller was actually given was dropped",
    );
    assert_eq!(dated.recorded_at, date(2026, 10, 11));

    // Both survive the journey back out of the store, which is the only
    // thing that says the column exists rather than the value being echoed.
    assert_eq!(read_back(store, &subject, &vague.id).await, vague);
    assert_eq!(read_back(store, &subject, &dated.id).await, dated);
}

/// **A claim's span covers every day it ran, and a single day still covers
/// only itself.**
///
/// ⛔️ **PAIRED, for the same reason [`a_claim_can_say_nothing_about_when_the_thing_happened`]
/// is:** a span read back wrong would pass this on its own if nothing also
/// proved the ordinary single-day shape is untouched by the column beside it.
/// Without the single-day half, a store that always answered `true` — or
/// that dropped `happened_at` down to the day range 101 started reading —
/// would still look correct here.
pub async fn a_claims_span_covers_every_day_it_ran<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-spanned");
    ensure(store, &subject).await;

    let festival = capture(
        store,
        NewFact {
            happened_at: Some(date(2026, 4, 18)),
            happened_through: Some(date(2026, 4, 20)),
            ..NewFact::about(subject.clone(), "ran the whole weekend", date(2026, 4, 22))
        },
    )
    .await;
    assert_eq!(festival.happened_at, Some(date(2026, 4, 18)));
    assert_eq!(
        festival.happened_through,
        Some(date(2026, 4, 20)),
        "the far end of the span was dropped",
    );
    assert_eq!(
        read_back(store, &subject, &festival.id).await,
        festival,
        "the span did not survive the journey back out of the store",
    );

    // The positive: every day from the start through the end.
    for day in [date(2026, 4, 18), date(2026, 4, 19), date(2026, 4, 20)] {
        assert!(
            festival.happened_covers(day),
            "{day} is inside the span and must be covered",
        );
    }
    // The negative: a window that misses the span entirely must not find it —
    // the day before it opened and the day after it closed.
    for day in [date(2026, 4, 17), date(2026, 4, 21)] {
        assert!(
            !festival.happened_covers(day),
            "{day} is outside the span and must not be covered",
        );
    }

    // **Unchanged**: a single-day claim, no far end at all, covers only that
    // one day — before this column existed and after, the same answer.
    let errand = capture(
        store,
        NewFact {
            happened_at: Some(date(2026, 4, 18)),
            ..NewFact::about(subject.clone(), "picked up the badges", date(2026, 4, 22))
        },
    )
    .await;
    assert_eq!(
        errand.happened_through, None,
        "an ordinary single-day claim must not grow a far end nobody gave it",
    );
    assert_eq!(
        read_back(store, &subject, &errand.id).await,
        errand,
        "a single-day claim did not survive the journey back out of the store",
    );
    assert!(errand.happened_covers(date(2026, 4, 18)));
    for day in [date(2026, 4, 17), date(2026, 4, 19)] {
        assert!(
            !errand.happened_covers(day),
            "a single-day claim must not cover {day}",
        );
    }
}

/// **An end with no start names a span nobody can read, on a fresh claim.**
///
/// The same shape `ValueType::DateRange` was built to prevent for a
/// caller-declared key — a range with no start half is refused there too.
/// A NAMED field gets the same floor.
pub async fn happened_through_with_no_start_is_refused<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-openended");
    ensure(store, &subject).await;

    let err = store
        .capture(NewFact {
            happened_through: Some(date(2026, 4, 20)),
            ..NewFact::about(subject, "an end with no start", date(2026, 4, 22))
        })
        .await
        .expect_err("an end with no start must be refused rather than written");
    assert!(
        matches!(&err, MemoryError::InvalidFact(said) if said.contains("happened_at")),
        "the refusal must name the way forward: {err:?}",
    );
}

/// **The hard half: whether the record's OWN state, not just the patch, is
/// what a caller is held to.**
///
/// ⛔️ **PAIRED, and the positive is the one that matters most.** A strict fix
/// that refuses whenever the patch's own `happened_at` argument is absent
/// would break the ordinary case — learning a trip's end after its start was
/// already on file — which this proves succeeds. The negative proves the
/// other direction still catches what it must: clearing the start out from
/// under a standing end.
pub async fn a_patch_may_widen_a_standing_start_but_not_orphan_one<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-openended");
    ensure(store, &subject).await;

    let claim = capture(
        store,
        NewFact {
            happened_at: Some(date(2026, 4, 18)),
            ..NewFact::about(subject, "the trip started", date(2026, 4, 22))
        },
    )
    .await;
    let address = claim.address();

    // The case that matters most: an end alone, against a start already on
    // the record, is an ordinary edit and must succeed.
    let widened = store
        .update_fact(
            &address,
            FactPatch {
                happened_through: Some(date(2026, 4, 20)),
                ..Default::default()
            },
        )
        .await
        .expect("update_fact ok")
        .written()
        .expect("a start already stands, so an end alone is a valid edit");
    assert_eq!(widened.happened_at, Some(date(2026, 4, 18)));
    assert_eq!(widened.happened_through, Some(date(2026, 4, 20)));

    // Taking the start off a claim that still carries an end would leave the
    // end orphaned — refused, not silently accepted.
    let err = store
        .update_fact(
            &address,
            FactPatch {
                clear_happened_at: true,
                ..Default::default()
            },
        )
        .await
        .expect_err("clearing the start out from under a standing end must be refused");
    assert!(
        matches!(&err, MemoryError::InvalidFact(said) if said.contains("happened_at")),
        "the refusal must name the way forward: {err:?}",
    );
}

/// **The day a thing happened is versioned with the rest of the claim.**
///
/// A claim is a projection over its writes and each write carries the whole
/// claim. **Without this column on the substrate, a claim that GAINED a day
/// in a later edit would read exactly like one that always had it** — and
/// taking a guessed day back would leave no trace at all, which is the
/// repair this split exists to make possible.
///
/// **Three writes and three readings**: silent, then dated, then silent
/// again. The middle one is what a chain of identical values could not
/// produce.
pub async fn the_day_a_thing_happened_is_versioned_like_the_rest_of_the_claim<M: Memory>(
    store: &M,
) {
    let subject = EntityId::person("person:contract-pumpback");
    ensure(store, &subject).await;

    let claim = capture(
        store,
        NewFact::about(subject.clone(), "brought the pump back", date(2026, 10, 11)),
    )
    .await;
    let address = claim.address();

    // Learned late: an ordinary edit, which is how a day usually arrives.
    store
        .update_fact(
            &address,
            FactPatch {
                happened_at: Some(date(2026, 8, 15)),
                ..Default::default()
            },
        )
        .await
        .expect("update_fact ok")
        .written()
        .expect("the guard waves it through");

    // …and taken back off, because it turned out to be somebody's estimate.
    store
        .update_fact(
            &address,
            FactPatch {
                clear_happened_at: true,
                ..Default::default()
            },
        )
        .await
        .expect("update_fact ok")
        .written()
        .expect("the guard waves it through");

    let chain = store
        .claim_history(&address)
        .await
        .expect("the claim's history reads");
    let said: Vec<Option<Date>> = chain.iter().map(|w| w.happened_at).collect();
    assert_eq!(
        said,
        vec![None, Some(date(2026, 8, 15)), None],
        "the substrate did not keep what each write said about the day",
    );

    // The claim as it stands says nothing, which is what the last write
    // said — so the projection and the chain agree.
    let now = read_back(store, &subject, &claim.id).await;
    assert_eq!(now.happened_at, None);
}

pub async fn preserves_all_fields<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-fields");
    // A claim that is really there: `derived_from` names one, and naming
    // one that does not exist is refused rather than stored.
    let source = capture(
        store,
        NewFact::about(subject.clone(), "mentioned the café", date(2026, 3, 8)),
    )
    .await
    .address();
    assert_eq!(
        source.to_string(),
        "person:contract-fields#f1",
        "the first claim on an entity is addressed f1, and that address is what a link carries"
    );
    let new = NewFact {
        subject: subject.clone(),
        content: "prefers a café table".into(),
        details: Some("mentioned it twice".into()),
        provenance: Provenance::Testimony,
        standing: Some(Standing::Open),
        status: FactStatus::Active,
        recorded_at: date(2026, 3, 9),
        happened_at: Some(date(2026, 3, 7)),
        happened_through: Some(date(2026, 3, 8)),
        edge: None,
        fields: [("seats".to_string(), "2".to_string())]
            .into_iter()
            .collect(),
        refs: vec![subject.clone()],
        derived_from: Some(source.clone()),
        stale_after: None,
    };
    let captured = capture(store, new).await;
    assert_eq!(captured.subject, subject);
    assert_eq!(captured.content, "prefers a café table");
    assert_eq!(captured.details.as_deref(), Some("mentioned it twice"));
    assert_eq!(captured.provenance, Provenance::Testimony);
    assert_eq!(captured.standing, Standing::Open);
    assert_eq!(captured.recorded_at, date(2026, 3, 9));
    // **The two dates are stored apart and neither takes the other's
    // value.** A store that kept one column would answer this with the
    // claim's own day and look correct until somebody read it.
    assert_eq!(captured.happened_at, Some(date(2026, 3, 7)));
    assert_eq!(captured.happened_through, Some(date(2026, 3, 8)));
    assert_eq!(captured.fields.get("seats").map(String::as_str), Some("2"));
    assert_eq!(captured.refs, vec![subject.clone()]);
    assert_eq!(captured.derived_from, Some(source));

    let seen = read_back(store, &subject, &captured.id).await;
    assert_eq!(seen, captured);
}

/// A raw pipe in content survives the round-trip (it must be escaped in the
/// table, not split into extra cells) — byte-identical.
pub async fn pipe_in_content_round_trips<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-pipe");
    let captured = capture(
        store,
        NewFact {
            details: Some("noted a|b in the margin".into()),
            ..NewFact::about(
                subject.clone(),
                "reads a|b|c pipe notation",
                date(2026, 7, 24),
            )
        },
    )
    .await;
    assert_eq!(captured.content, "reads a|b|c pipe notation");
    let seen = read_back(store, &subject, &captured.id).await;
    assert_eq!(seen, captured);
}

/// A backslash in content or details survives the round-trip — byte-identical.
///
/// A contract case rather than a codec unit test, because it is a claim
/// about STORAGE: a fake keeping bytes verbatim answers yes whatever the
/// codec does, and only the real store can say the escape holds.
pub async fn a_backslash_in_content_round_trips<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-backslash");
    let captured = capture(
        store,
        NewFact {
            details: Some(r#"quoted it as \"exactly this\""#.into()),
            ..NewFact::about(
                subject.clone(),
                r"the path is c:\dir\file and a trailing \",
                date(2026, 7, 24),
            )
        },
    )
    .await;
    assert_eq!(
        captured.content,
        r"the path is c:\dir\file and a trailing \"
    );
    assert_eq!(
        captured.details.as_deref(),
        Some(r#"quoted it as \"exactly this\""#)
    );
    let seen = read_back(store, &subject, &captured.id).await;
    assert_eq!(seen, captured);
}

/// Both provenance values survive independently — the regression guard for
/// the collision that dropped/corrupted facts when provenance was folded
/// into content. Testimony must come back testimony, inference inference.
pub async fn both_provenances_survive<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-provenance");
    let testi = capture(
        store,
        NewFact {
            provenance: Provenance::Testimony,
            ..NewFact::about(subject.clone(), "speaks two languages", date(2026, 1, 1))
        },
    )
    .await;
    let infer = capture(
        store,
        NewFact {
            // Content that ends in the human ❓ glyph must NOT be read as
            // inference-by-marker — provenance is its own column now.
            provenance: Provenance::Inference,
            ..NewFact::about(
                subject.clone(),
                "might prefer mornings ❓",
                date(2026, 1, 2),
            )
        },
    )
    .await;

    let seen_testi = read_back(store, &subject, &testi.id).await;
    let seen_infer = read_back(store, &subject, &infer.id).await;
    assert_eq!(seen_testi.provenance, Provenance::Testimony);
    assert_eq!(seen_testi.content, "speaks two languages");
    assert_eq!(seen_infer.provenance, Provenance::Inference);
    assert_eq!(seen_infer.content, "might prefer mornings ❓");
}

/// Edge whitespace is not significant: capture normalizes it, and the
/// returned fact is byte-identical to what recall reads back.
pub async fn edge_whitespace_is_normalized<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-whitespace");
    let captured = capture(
        store,
        NewFact::about(subject.clone(), "   likes espresso   ", date(2026, 7, 24)),
    )
    .await;
    assert_eq!(
        captured.content, "likes espresso",
        "capture must normalize edge whitespace"
    );
    let seen = read_back(store, &subject, &captured.id).await;
    assert_eq!(seen, captured, "recalled fact must be byte-identical");
}

/// Two distinct captures are both recallable, each under its own id.
pub async fn multiple_facts_all_recallable<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-multi");
    let a = capture(
        store,
        NewFact::about(subject.clone(), "plays go", date(2026, 7, 1)),
    )
    .await;
    let b = capture(
        store,
        NewFact::about(subject.clone(), "learning Rust", date(2026, 7, 2)),
    )
    .await;
    assert_ne!(a.id, b.id, "each fact must get its own id");
    assert_eq!(read_back(store, &subject, &a.id).await.content, "plays go");
    assert_eq!(
        read_back(store, &subject, &b.id).await.content,
        "learning Rust"
    );
}

/// Facts about one entity never leak into another's recall — each subject's
/// facts are isolated (a per-entity doc, in the real adapter).
pub async fn subjects_are_isolated<M: Memory>(store: &M) {
    let solo = EntityId::person("person:contract-solo");
    let duet = EntityId::person("person:contract-duet");
    capture(
        store,
        NewFact::about(solo.clone(), "solo fact", date(2026, 7, 1)),
    )
    .await;
    capture(
        store,
        NewFact::about(duet.clone(), "duet fact", date(2026, 7, 1)),
    )
    .await;

    let solo_facts = store.recall(&solo).await.expect("recall solo");
    assert!(
        solo_facts.iter().all(|f| f.subject == solo),
        "recall(solo) must only return solo's facts"
    );
    assert!(solo_facts.iter().any(|f| f.content == "solo fact"));
    assert!(
        !solo_facts.iter().any(|f| f.content == "duet fact"),
        "duet's fact must not appear under solo"
    );
}

/// An adversarial subject id — one carrying a pipe, a newline, a markdown
/// header, or a fence — is rejected at capture, never written. This is the
/// injection guard: a forged subject must not be able to fabricate a fact
/// row, a table, or a header in someone's doc.
pub async fn malicious_subjects_are_rejected<M: Memory>(store: &M) {
    for bad in [
        "person:a|b",
        "person:a\nb",
        "person:a\n### forged",
        "person:a`b`",
        "person:a b",
        "receipt:not-a-kind",
    ] {
        let err = store
            .capture(NewFact::about(
                EntityId(bad.into()),
                "should never be stored",
                date(2026, 7, 24),
            ))
            .await
            .expect_err("a malicious subject must be rejected");
        assert!(
            matches!(err, MemoryError::InvalidSubject(_)),
            "expected InvalidSubject for {bad:?}, got {err:?}"
        );
    }
}

/// Recalling an entity nobody ever created is a MISS — `UnknownEntity`,
/// naming the attempt with the near candidates that explain it — never an
/// empty success. An empty page and a nonexistent entity are different
/// facts, and the production smoke test caught them dressed identically: a
/// caller told a bad handle "reads fine, no facts" can never repair it.
/// An entity that EXISTS with no facts still recalls empty, and nothing is
/// created either way.
pub async fn recall_unknown_is_a_miss_not_an_empty_page<M: Memory>(store: &M) {
    let never = EntityId::person("person:contract-never-captured");
    let err = store
        .recall(&never)
        .await
        .expect_err("an unknown entity must not read as an empty page");
    match &err {
        MemoryError::UnknownEntity { attempted, .. } => {
            assert_eq!(attempted, &never.to_string());
        }
        other => panic!("expected UnknownEntity, got {other:?}"),
    }

    // A typo'd handle explains itself: the miss carries its neighbour.
    let real = EntityId::person("person:contract-orient");
    ensure(store, &real).await;
    let typo = EntityId::person("person:contract-orjent");
    let err = store
        .recall(&typo)
        .await
        .expect_err("a typo'd handle is a miss");
    match &err {
        MemoryError::UnknownEntity { nearest, .. } => {
            assert!(
                nearest.iter().any(|m| m.handle == real),
                "the near candidate must surface: {err:?}"
            );
        }
        other => panic!("expected UnknownEntity, got {other:?}"),
    }

    // Exists-but-empty is the OTHER case, and it still reads fine.
    let facts = store
        .recall(&real)
        .await
        .expect("an existing entity's empty page reads");
    assert!(
        facts.is_empty(),
        "no facts were created along the way: {facts:?}"
    );
}

// --- the entity model ----------------------------------------------------

/// A fact can be about any kind that memory owns, not just people — and
/// each lands in its own home, addressable under its own handle.
///
/// ⛔️ **`session` is the exception, and it is a real one rather than a gap
/// in this case.** A session is addressable so a bot can ask the graph
/// about its own past runs, and it is written through the session verbs
/// alone: its life is a state machine — bound to its bot, wrapped once and
/// never reopened, only an abandoned run walking back — that a memory write
/// would step straight past. **Selectable is not writable, and this kind is
/// where those two part company.**
pub async fn every_kind_holds_facts<M: Memory>(store: &M) {
    for kind in EntityKind::ALL
        .into_iter()
        .filter(|kind| *kind != EntityKind::SESSION)
    {
        let subject = EntityId::new(kind, format!("contract-kind-{kind}"));
        let captured = capture(
            store,
            NewFact::about(subject.clone(), format!("a {kind} fact"), date(2026, 7, 24)),
        )
        .await;
        assert_eq!(captured.subject.kind(), Some(kind));
        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(seen.content, format!("a {kind} fact"));
    }
}

/// `add_entity` writes an entity of any kind, and the read path returns it
/// with every frontmatter field intact.
pub async fn add_entity_reads_back<M: Memory>(store: &M) {
    let id = EntityId("project:contract-atlas".into());
    let added = add(
        store,
        NewEntity {
            crm: Some("card:874".into()),
            boot: Boot::Always,
            ..NewEntity::new(id.clone(), "Atlas", "user-named")
        },
    )
    .await;
    assert_eq!(added.kind, EntityKind::PROJECT);

    let seen = read_entity(store, &id).await;
    assert_eq!(seen, added, "the listed entity must be byte-identical");
    assert_eq!(seen.name, "Atlas");
    assert_eq!(seen.source, "user-named");
    assert_eq!(seen.crm.as_deref(), Some("card:874"));
    assert_eq!(seen.boot, Boot::Always);
}

// --- the tree ------------------------------------------------------------

/// **An entity can name a parent, and it survives the read path.** A root
/// names none, which is what most entities are.
/// **Who points here, read from the far end.**
///
/// A reference key makes a value a link, and a store has to answer that
/// link from the thing it points AT — otherwise the only way to find the
/// records naming something is to read every record there is.
///
/// Every store answers the same three ways: a record naming the handle
/// comes back whatever key it used, a record naming something else does
/// not, and a key that was overwritten stops pointing. **The third is what
/// separates reading the writes from reading the records**: the write row
/// that named the old target is still in the store, and the record no
/// longer carries it.
/// **The two clocks, and they are allowed to disagree.**
///
/// A claim's date says when it is TRUE OF. The stamp says when this store
/// took it in. Neither is checked against the other, because both
/// disagreements are ordinary: a booking is true of a day that has not
/// arrived, and a backfill carries years of records that are true of days
/// long past and land this morning.
///
/// **The store writes the stamp.** Nothing above it can, which is what
/// makes *jojobot knew this then* something nobody can claim after the
/// fact.
pub async fn a_claim_carries_when_it_was_taken_in<M: Memory>(store: &M) {
    let subject = EntityId("person:contract-clocks".into());
    add(
        store,
        NewEntity::new(subject.clone(), "Contract Clocks", "contract-fixture"),
    )
    .await;

    let before = jiff::Timestamp::now();
    // True of a day long past, taken in now: the backfill.
    let backfilled = capture(
        store,
        NewFact::about(subject.clone(), "moved here", date(2022, 3, 1)),
    )
    .await;
    // True of a day that has not arrived, taken in now: the booking.
    let booked = capture(
        store,
        NewFact::about(subject.clone(), "flies out", date(2027, 6, 12)),
    )
    .await;
    let after = jiff::Timestamp::now();

    for (what, fact, held) in [
        ("the backfilled claim", &backfilled, date(2022, 3, 1)),
        ("the booking", &booked, date(2027, 6, 12)),
    ] {
        assert_eq!(fact.recorded_at, held, "{what} lost the day it is true of");
        let stamp = fact
            .inserted_at
            .unwrap_or_else(|| panic!("{what} came back with no stamp: {fact:?}"));
        assert!(
            before <= stamp && stamp <= after,
            "{what} was stamped outside the call that wrote it: {stamp}",
        );
    }

    // **Read back, because a stamp the write invented and the store did not
    // keep is not a stamp.**
    let held = store
        .recall(&subject)
        .await
        .expect("the claims read back")
        .into_iter()
        .map(|fact| (fact.content, fact.recorded_at, fact.inserted_at))
        .collect::<Vec<_>>();
    assert_eq!(
        held,
        vec![
            (
                "moved here".to_string(),
                date(2022, 3, 1),
                backfilled.inserted_at
            ),
            (
                "flies out".to_string(),
                date(2027, 6, 12),
                booked.inserted_at
            ),
        ],
        "a read gives back both clocks, unchanged",
    );

    // **An edit does not re-stamp.** Correcting a claim's wording is not
    // the moment this store learned it.
    let edited = edit(
        store,
        &backfilled.address(),
        FactPatch {
            content: Some("moved here in the spring".to_string()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        edited.inserted_at, backfilled.inserted_at,
        "an edit re-stamped the record with the moment somebody corrected it",
    );

    // **The day somebody set to look again survives storage**, and comes
    // off when an edit says so. It is the caller's judgement, unlike the
    // stamp above, so a caller may move it and remove it.
    let watched = capture(
        store,
        NewFact {
            stale_after: Some(date(2026, 12, 1)),
            ..NewFact::about(
                subject.clone(),
                "the rate is fixed for now",
                date(2026, 8, 1),
            )
        },
    )
    .await;
    assert_eq!(watched.stale_after, Some(date(2026, 12, 1)));
    assert_eq!(
        read_back(store, &subject, &watched.id).await.stale_after,
        Some(date(2026, 12, 1)),
        "the day did not survive the store",
    );
    let cleared = edit(
        store,
        &watched.address(),
        FactPatch {
            clear_stale_after: true,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        cleared.stale_after, None,
        "a claim nobody has to look at again still carries a day",
    );

    // **A retraction is a record in its own right**, so it is taken in at
    // the moment it is written — and the record it takes back keeps the
    // moment IT was taken in, because a retraction says a claim should not
    // have been recorded, never that it arrived later than it did.
    let taken_back = store
        .retract(
            &watched.address(),
            Some("written in error"),
            date(2026, 8, 2),
        )
        .await
        .expect("the retraction lands");
    assert!(
        taken_back.record.inserted_at.is_some(),
        "the account of a retraction carries no stamp: {:?}",
        taken_back.record,
    );
    assert_eq!(
        taken_back.retracted.inserted_at, watched.inserted_at,
        "taking a claim back re-stamped it with the moment somebody took it back",
    );
}

/// **Lineage, walked from the source's end.**
///
/// A claim names what it was worked out from; this is the question nobody
/// could ask — **what was built on this** — and it is asked the moment a
/// claim is taken back.
///
/// Three legs, and the second two are what make the first mean anything: a
/// claim derived from this one is found · a claim about the same subject
/// that was derived from nothing is NOT found there · and **a claim that
/// stops resting on it stops being found**, which no test that only adds
/// records can catch.
/// **A folded value says which claim it came from and who backs it.**
///
/// The same string arrives whether the user said it this morning or an
/// assistant guessed it two years ago, and a reader that cannot tell them
/// apart has to treat both alike. **Under the default fold exactly one
/// write wins**, so the honest answer is that claim's own certainty.
///
/// Three legs. The winning write's certainty is reported · the loser's is
/// NOT, which is what stops this passing on a build that reports whatever
/// it finds first · and **the answer changes when the winning write
/// changes**, which no test that only adds one claim can reach.
/// **A claim an agent read out of a system of record, and what it costs to
/// say so.**
///
/// It is neither the user's word nor a guess: filing a statement as a
/// derivation makes a system of record read as a hypothesis, and filing it
/// as testimony puts words in somebody's mouth. **What makes it usable a
/// year later is the attribution**, so the claim is refused when it names
/// no system — the check is on the source and never on the standing.
///
/// Four legs, and each of the last three is why the first means anything:
/// unattributed is refused · attributed lands and **reads back settled**,
/// which is the whole point of the value · an ordinary derivation with no
/// source is untouched, so the rule reaches only the claims it is about ·
/// and **a claim that stops being machine-read stops being held to it**.
pub async fn a_machine_read_claim_names_what_it_was_read_from<M: Memory>(store: &M) {
    let subject = EntityId("person:contract-observed".into());
    add(
        store,
        NewEntity::new(subject.clone(), "Contract Observed", "contract-fixture"),
    )
    .await;

    let unattributed = store
        .capture(NewFact {
            provenance: Provenance::Observation,
            ..NewFact::about(subject.clone(), "the invoice is paid", date(2026, 8, 1))
        })
        .await
        .expect_err("a machine-read claim with no source is refused");
    assert!(
        matches!(unattributed, MemoryError::UnsourcedObservation),
        "the refusal is about something else: {unattributed:?}",
    );

    let read = capture(
        store,
        NewFact {
            provenance: Provenance::Observation,
            fields: [
                ("read_from".to_string(), "the ledger app".to_string()),
                ("read_ref".to_string(), "invoice-4471".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(subject.clone(), "the invoice is paid", date(2026, 8, 1))
        },
    )
    .await;
    assert_eq!(read.provenance, Provenance::Observation);
    assert_eq!(
        read.standing,
        Standing::Settled,
        "a confident read of a system of record reads back as a hypothesis",
    );
    assert_eq!(
        read_back(store, &subject, &read.id).await.provenance,
        Provenance::Observation,
        "the provenance did not survive the store",
    );

    // **The rule reaches only the claims it is about.** An ordinary
    // derivation names no source and is written as it always was — without
    // this, the refusal above passes against a store demanding a source
    // from everything.
    capture(
        store,
        NewFact::about(
            subject.clone(),
            "so the account is square",
            date(2026, 8, 2),
        ),
    )
    .await;

    // **The leg a change reaches.** Edit the machine-read claim down to a
    // derivation and the source stops being required: what is held to the
    // rule is the claim's provenance now, not the one it was written with.
    let softened = edit(
        store,
        &read.address(),
        FactPatch {
            provenance: Some(Provenance::Inference),
            clear_fields: vec!["read_from".to_string(), "read_ref".to_string()],
            ..Default::default()
        },
    )
    .await;
    assert_eq!(softened.provenance, Provenance::Inference);
    assert!(
        !softened.fields.contains_key("read_from"),
        "the source stayed on a claim that is no longer machine-read: {softened:?}",
    );

    // **And the leg that reaches the same change the other way.** The rule
    // is about what a claim IS, so it binds every path that can make one a
    // machine read — not only the one that writes it first. A guard on
    // capture alone lets a guess become a system read of a system nobody
    // named, and the value outranks the honest guesses beside it.
    let guessed = capture(
        store,
        NewFact::about(
            subject.clone(),
            "the balance looks settled",
            date(2026, 8, 3),
        ),
    )
    .await;
    let hardened = store
        .update_fact(
            &guessed.address(),
            FactPatch {
                provenance: Some(Provenance::Observation),
                ..Default::default()
            },
        )
        .await
        .expect_err("a claim moved to a machine read with no source is refused");
    assert!(
        matches!(hardened, MemoryError::UnsourcedObservation),
        "the refusal is about something else: {hardened:?}",
    );
    assert_eq!(
        read_back(store, &subject, &guessed.id).await.provenance,
        Provenance::Inference,
        "the refused edit moved the claim anyway",
    );

    // The positive it rests on: the same move, naming the system. Without
    // this the refusal passes on a store that turns every edit away.
    let attributed = edit(
        store,
        &guessed.address(),
        FactPatch {
            provenance: Some(Provenance::Observation),
            fields: [("read_from".to_string(), "the ledger app".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(attributed.provenance, Provenance::Observation);

    // **A claim that already names its system may be moved without naming
    // it again.** What the rule protects is an attribution ON THE RECORD,
    // and this record has one — so the question is asked of the claim as it
    // will stand, never of the patch alone.
    let softened_again = edit(
        store,
        &attributed.address(),
        FactPatch {
            provenance: Some(Provenance::Inference),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(softened_again.provenance, Provenance::Inference);
    let re_hardened = edit(
        store,
        &attributed.address(),
        FactPatch {
            provenance: Some(Provenance::Observation),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        re_hardened.provenance,
        Provenance::Observation,
        "a claim carrying its source already does not have to name it twice",
    );

    // **And the same rule reached from the fields.** Taking the source off
    // a claim that stays a machine read leaves exactly the state the rule
    // forbids, so it is refused for the same reason.
    let stripped = store
        .update_fact(
            &attributed.address(),
            FactPatch {
                clear_fields: vec!["read_from".to_string()],
                ..Default::default()
            },
        )
        .await
        .expect_err("taking the source off a machine read is refused");
    assert!(
        matches!(stripped, MemoryError::UnsourcedObservation),
        "the refusal is about something else: {stripped:?}",
    );
}

pub async fn a_folded_value_says_who_backs_it<M: Memory>(store: &M) {
    let subject = EntityId("person:contract-backing".into());
    add(
        store,
        NewEntity::new(subject.clone(), "Contract Backing", "contract-fixture"),
    )
    .await;

    // A guess first, then the user's own word: the newest write wins, and
    // it is the one whose certainty the answer must carry.
    capture(
        store,
        NewFact {
            provenance: Provenance::Inference,
            fields: [("rent".to_string(), "900".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                subject.clone(),
                "worked it out from the listing",
                date(2026, 5, 1),
            )
        },
    )
    .await;
    let stated = capture(
        store,
        NewFact {
            provenance: Provenance::Testimony,
            standing: Some(Standing::Settled),
            fields: [("rent".to_string(), "950".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                subject.clone(),
                "he said what the rent is",
                date(2026, 6, 1),
            )
        },
    )
    .await;

    let backing = |entity: EntityId| async move {
        store
            .backing(&entity)
            .await
            .expect("a store says what backs a folded value")
    };
    let held = backing(subject.clone()).await;
    let rent = held
        .get("rent")
        .unwrap_or_else(|| panic!("the folded value says nothing about its claim: {held:?}"));
    assert_eq!(rent.fact, stated.id, "the backing names the losing claim");
    assert_eq!(
        (rent.provenance, rent.standing),
        (Provenance::Testimony, Standing::Settled),
        "the value the user stated reads as a guess, or the other way about",
    );

    // **The leg only a change reaches.** A later guess wins the key, and the
    // certainty reported moves with it — a build that read the claim once
    // would go on reporting testimony for a value nobody stated.
    capture(
        store,
        NewFact {
            provenance: Provenance::Inference,
            fields: [("rent".to_string(), "975".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                subject.clone(),
                "the listing went up again",
                date(2026, 7, 1),
            )
        },
    )
    .await;
    let held = backing(subject.clone()).await;
    let rent = held.get("rent").expect("the key is still held");
    assert_eq!(
        (rent.provenance, rent.standing),
        (Provenance::Inference, Standing::Open),
        "the certainty did not move with the write that won: {held:?}",
    );
}

/// **A summed key has no backing, because no single write won it.**
///
/// [`FieldBacking`] answers which claim a folded value came from and how
/// sure that claim was. A key whose writes are added together has no such
/// claim: the value is every write at once, so naming one of them attaches
/// a provenance and a standing that nobody stated to a number nobody wrote.
/// **That is a claim the store invents**, which is the one thing it must
/// never do.
///
/// **Three halves, and each covers a way the other two pass on a wrong
/// build.** The total says the key really is summed, or the case is about
/// an ordinary key. The absent backing is the claim. The key beside it,
/// folded the ordinary way, still reports its claim — without it the case
/// passes against a store that reports no backing at all.
///
/// The three writes carry three certainties, so a build that names one of
/// them cannot name a right answer by accident.
pub async fn a_summed_key_has_no_backing_to_report<M: Memory>(store: &M) {
    use crate::memory::types::{DeclaredType, Field, Fold, ValueType};
    let subject = EntityId("person:contract-tallied".into());
    add(
        store,
        NewEntity::new(subject.clone(), "Contract Tallied", "contract-fixture"),
    )
    .await;
    store
        .declare_type(DeclaredType::new(
            "contract-tallying",
            vec![
                Field::summing("laps_swum").needed(),
                Field::required("pool", ValueType::Text),
            ],
        ))
        .await
        .expect("declaring a type should succeed");
    assert_eq!(
        store
            .declared_types()
            .await
            .expect("the roster reads")
            .iter()
            .find(|t| t.name == "contract-tallying")
            .and_then(|t| t.field("laps_swum"))
            .map(|f| f.folds),
        Some(Fold::Sum),
        "the fold survives the store, or nothing below is about a counter",
    );

    for (nth, (provenance, standing)) in [
        (Provenance::Testimony, Standing::Settled),
        (Provenance::Inference, Standing::Open),
        (Provenance::Observation, Standing::Settled),
    ]
    .into_iter()
    .enumerate()
    {
        let mut fields: std::collections::BTreeMap<String, String> = [
            ("laps_swum".to_string(), "1".to_string()),
            ("pool".to_string(), "the lido".to_string()),
        ]
        .into_iter()
        .collect();
        // A machine read names where it was read, whatever else it carries.
        if provenance == Provenance::Observation {
            fields.insert("read_from".to_string(), "the lane counter".to_string());
        }
        capture(
            store,
            NewFact {
                provenance,
                standing: Some(standing),
                fields,
                ..NewFact::about(
                    subject.clone(),
                    format!("a lap, number {}", nth + 1),
                    date(2026, 7, 1),
                )
            },
        )
        .await;
    }

    let held = store
        .fields(&subject)
        .await
        .expect("a store says what a thing holds");
    assert_eq!(
        held.get("laps_swum").map(String::as_str),
        Some("3"),
        "the writes are added together, or this case is not about a counter: {held:?}",
    );

    let backing = store
        .backing(&subject)
        .await
        .expect("a store says what backs a folded value");
    assert!(
        !backing.contains_key("laps_swum"),
        "a summed value names one claim as its backing, so a number nobody wrote carries a \
         certainty nobody stated: {backing:?}",
    );
    assert!(
        backing.contains_key("pool"),
        "…and the key folded the ordinary way still reports its claim, or this case would \
         pass against a store that reports no backing at all: {backing:?}",
    );
}

pub async fn a_claims_lineage_is_walkable_from_its_source<M: Memory>(store: &M) {
    let subject = EntityId("person:contract-lineage".into());
    add(
        store,
        NewEntity::new(subject.clone(), "Contract Lineage", "contract-fixture"),
    )
    .await;
    let source = capture(
        store,
        NewFact::about(
            subject.clone(),
            "the ferry moved to the north pier",
            date(2026, 4, 1),
        ),
    )
    .await;
    let other = capture(
        store,
        NewFact::about(
            subject.clone(),
            "the bridge is closed on Sundays",
            date(2026, 4, 1),
        ),
    )
    .await;
    let built = capture(
        store,
        NewFact {
            derived_from: Some(source.address()),
            ..NewFact::about(
                subject.clone(),
                "so the crossing is longer",
                date(2026, 4, 2),
            )
        },
    )
    .await;

    let standing_on = |address: FactAddress| async move {
        store
            .built_on(&address)
            .await
            .expect("a store answers what was built on a claim")
            .into_iter()
            .map(|fact| fact.address().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        standing_on(source.address()).await,
        vec![built.address().to_string()],
        "the claim built on this one is not found from its source's end",
    );
    assert!(
        standing_on(other.address()).await.is_empty(),
        "a claim nobody built on answers with the claims built on something else",
    );

    // **The leg that only a change can prove.** Point the derived claim at
    // the other source and it must leave the first one's answer — a walk
    // that read the pointer once and never again would keep it there.
    edit(
        store,
        &built.address(),
        FactPatch {
            derived_from: Some(other.address()),
            ..Default::default()
        },
    )
    .await;
    assert!(
        standing_on(source.address()).await.is_empty(),
        "a claim that stopped resting on this one is still found under it",
    );
    assert_eq!(
        standing_on(other.address()).await,
        vec![built.address().to_string()],
        "…and it is not found under the source it now names",
    );

    // **Archive is a visibility switch, not a validity gate.** A claim that
    // was taken back is still there — retraction is a state, not a
    // deletion — and it may still be what another claim rests on. Refusing
    // it would make archiving decide what may be cited, which is the
    // validity gate this store just stopped being.
    let withdrawn = capture(
        store,
        NewFact::about(subject.clone(), "the pier is open again", date(2026, 4, 3)),
    )
    .await;
    store
        .retract(
            &withdrawn.address(),
            Some("misread the notice"),
            date(2026, 4, 4),
        )
        .await
        .expect("the retraction lands");
    let resting_on_archived = capture(
        store,
        NewFact {
            derived_from: Some(withdrawn.address()),
            ..NewFact::about(
                subject.clone(),
                "so the crossing is short again",
                date(2026, 4, 5),
            )
        },
    )
    .await;
    assert_eq!(
        resting_on_archived.derived_from.map(|a| a.to_string()),
        Some(withdrawn.address().to_string()),
        "a claim citing an archived source must still be allowed to name it",
    );

    // **The same permission reached by an EDIT.** A pointer set on a claim
    // that already exists lands the store in the identical state a capture
    // would, so a check on one path alone leaves the other one open — and
    // the edit is the path a session takes when it works out where a claim
    // came from after writing it.
    let repointed = edit(
        store,
        &built.address(),
        FactPatch {
            derived_from: Some(withdrawn.address()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        repointed.derived_from.map(|a| a.to_string()),
        Some(withdrawn.address().to_string()),
        "an edit citing an archived source must still be allowed to name it",
    );
    assert_eq!(
        read_back(store, &subject, &built.id)
            .await
            .derived_from
            .map(|a| a.to_string()),
        Some(withdrawn.address().to_string()),
        "…and the pointer the edit set is what a later read sees",
    );

    // **The positive it depends on**: the same claim goes through when it
    // names a source that still stands. Without this, the refusals above
    // pass against a store that refuses every lineage pointer.
    capture(
        store,
        NewFact {
            derived_from: Some(other.address()),
            ..NewFact::about(
                subject.clone(),
                "so the timetable changed",
                date(2026, 4, 5),
            )
        },
    )
    .await;
}

pub async fn referring_to_answers_from_the_far_end<M: Memory>(store: &M) {
    let gate = EntityId("event:contract-winter-fest".into());
    let other = EntityId("event:contract-leaving-party".into());
    let holder = EntityId("person:contract-milhouse".into());
    let bystander = EntityId("person:contract-otto".into());
    for (id, name) in [
        (&gate, "Contract Winter Fest"),
        (&other, "Contract Leaving Party"),
        (&holder, "Contract Milhouse"),
        (&bystander, "Contract Otto"),
    ] {
        add(store, NewEntity::new(id.clone(), name, "contract-fixture")).await;
    }

    let held = capture(
        store,
        NewFact {
            fields: [("admits".to_string(), gate.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(holder.clone(), "holds a full pass", date(2026, 8, 1))
        },
    )
    .await;
    capture(
        store,
        NewFact {
            fields: [("admits".to_string(), other.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                bystander.clone(),
                "holds a pass to the other one",
                date(2026, 8, 1),
            )
        },
    )
    .await;

    let pointing = store
        .referring_to(&gate)
        .await
        .expect("a store answers who points here");
    assert_eq!(
        pointing
            .iter()
            .map(|fact| fact.address().to_string())
            .collect::<Vec<_>>(),
        vec![held.address().to_string()],
        "the record naming this handle comes back, and the one naming another does not",
    );

    // **A key pointed elsewhere stops pointing here.** The write that named
    // the gate is still in the store; the record is not carrying it any
    // more, and that is what the answer follows.
    edit(
        store,
        &held.address(),
        FactPatch {
            fields: [("admits".to_string(), other.to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        },
    )
    .await;
    assert!(
        store
            .referring_to(&gate)
            .await
            .expect("a store answers who points here")
            .is_empty(),
        "a key that was pointed somewhere else still points here",
    );

    // **Every status comes back, retracted included**, because this read
    // serves history as often as current truth. **A reader of current truth
    // has to filter for itself**, and it can only do that if the store says
    // what state each record is in rather than deciding for it.
    let withdrawn = capture(
        store,
        NewFact {
            fields: [("admits".to_string(), gate.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(bystander.clone(), "was given a pass", date(2026, 8, 1))
        },
    )
    .await;
    store
        .retract(&withdrawn.address(), Some("never issued"), date(2026, 8, 2))
        .await
        .expect("the retraction lands");
    let after = store
        .referring_to(&gate)
        .await
        .expect("a store answers who points here");
    let taken_back = after
        .iter()
        .find(|fact| fact.id == withdrawn.id)
        .unwrap_or_else(|| panic!("the withdrawn record is not in the answer: {after:?}"));
    assert_eq!(
        taken_back.status,
        FactStatus::Archived,
        "the answer hides what state a record is in, so no reader above can filter on it",
    );
}

pub async fn a_child_names_its_parent_and_reads_back<M: Memory>(store: &M) {
    let parent = EntityId("project:contract-monorail".into());
    let child = EntityId("project:contract-monorail-funding".into());

    let root = add(
        store,
        NewEntity::new(parent.clone(), "Contract Monorail", "contract-fixture"),
    )
    .await;
    assert_eq!(root.parent, None, "an entity under nothing is a root");

    // **A child named for its parent is a containment near miss**, and
    // rightly: `monorail-funding` beside `monorail` is exactly the shape
    // that is usually one thing filed twice. Here the two really are
    // different, so it goes over the screen the way a caller would say so —
    // read the refusal, hand its token back.
    let added = add_over_the_screen(
        store,
        NewEntity {
            parent: Some(parent.clone()),
            ..NewEntity::new(child.clone(), "Monorail Funding", "contract-fixture")
        },
    )
    .await;
    assert_eq!(added.parent.as_ref(), Some(&parent));

    let seen = read_entity(store, &child).await;
    assert_eq!(
        seen, added,
        "the listed child must be byte-identical, parent included"
    );
    assert_eq!(
        read_entity(store, &parent).await.parent,
        None,
        "…and the parent is still a root"
    );
}

/// **Children come back as handles, one level down.** Zooming is the whole
/// point: a parent read hands back the branch names and nothing else, so
/// the caller pays only for the branch it descends into. A grandchild is
/// not a child, and a leaf has none.
pub async fn children_are_handles_and_one_level_deep<M: Memory>(store: &M) {
    let root = EntityId("project:contract-springfield".into());
    let track = EntityId("project:contract-springfield-track".into());
    let cars = EntityId("project:contract-springfield-cars".into());
    let brakes = EntityId("project:contract-springfield-brakes".into());

    add(
        store,
        NewEntity::new(root.clone(), "Contract Springfield", "contract-fixture"),
    )
    .await;
    // Every child's handle contains its parent's, which is the containment
    // near miss doing its job — a branch named for its trunk is the shape
    // that is usually one thing filed twice. These are deliberate, so each
    // goes over the screen with the token its own refusal minted.
    for (id, name, under) in [
        (&track, "Springfield Track", &root),
        (&cars, "Springfield Cars", &root),
        (&brakes, "Springfield Brakes", &cars),
    ] {
        add_over_the_screen(
            store,
            NewEntity {
                parent: Some(under.clone()),
                ..NewEntity::new(id.clone(), name, "contract-fixture")
            },
        )
        .await;
    }

    let mut got = store
        .children(&root)
        .await
        .expect("children should succeed");
    got.sort();
    let mut want = vec![track.clone(), cars.clone()];
    want.sort();
    assert_eq!(
        got, want,
        "exactly the direct children — the grandchild is the next level's business"
    );
    assert_eq!(
        store
            .children(&cars)
            .await
            .expect("children should succeed"),
        vec![brakes.clone()],
        "the middle of the tree has children of its own"
    );
    assert_eq!(
        store
            .children(&brakes)
            .await
            .expect("children should succeed"),
        Vec::<EntityId>::new(),
        "a leaf has none, and says so with an empty list"
    );
}

/// **Editing a child does not orphan it.** Every write that rewrites a
/// whole document is a chance to drop a field nobody was thinking about,
/// and parentage is the newest and least-remembered one. A rename and a
/// prose replacement both go through here, because both rebuild the page
/// around the part they came to change.
pub async fn a_write_that_rewrites_a_child_leaves_it_where_it_was<M: Memory>(store: &M) {
    let parent = EntityId("project:contract-kwik-e".into());
    let child = EntityId("project:contract-kwik-e-squishee".into());
    add(
        store,
        NewEntity::new(parent.clone(), "Contract Kwik-E", "contract-fixture"),
    )
    .await;
    // A child named for its parent trips the containment screen; the two
    // are deliberately different, so it goes over with its own token.
    add_over_the_screen(
        store,
        NewEntity {
            parent: Some(parent.clone()),
            ..NewEntity::new(child.clone(), "Squishee Machine", "contract-fixture")
        },
    )
    .await;

    let renamed = store
        .update_entity(
            &child,
            EntityPatch {
                name: Some("Squishee Machine (the second one)".into()),
                crm: Some("card:552".into()),
                ..Default::default()
            },
        )
        .await
        .expect("update_entity should succeed")
        .written()
        .expect("the guard must not block an uncontested rename");
    assert_eq!(
        renamed.parent.as_ref(),
        Some(&parent),
        "a metadata edit rebuilds the record; the parent must survive it"
    );

    store
        .set_prose(&child, "The machine, and what it is for.")
        .await
        .expect("set_prose should succeed");
    assert_eq!(
        read_entity(store, &child).await.parent.as_ref(),
        Some(&parent),
        "…and so must a prose write, which rebuilds the page around it"
    );
    assert_eq!(
        store
            .children(&parent)
            .await
            .expect("children should succeed"),
        vec![child.clone()],
        "the parent still has it, which is the only place that would show"
    );
}

/// **A malformed parent is malformed, not merely unresolvable.** The two
/// refusals have different shapes and a caller branches on them
/// differently: a handle that is not a handle comes back an error, and one
/// that is a handle but resolves to nothing comes back blocked with
/// candidates. Collapsing the first into the second would answer "I don't
/// know that one" to a caller whose real problem is that it never wrote a
/// handle at all.
pub async fn a_parent_that_is_not_a_handle_is_refused_before_the_guard<M: Memory>(store: &M) {
    let child = EntityId("project:contract-bad-parent".into());
    for bad in ["Some Project", "person:", "notakind:atlas", "person:Alpha"] {
        let err = store
            .add_entity(NewEntity {
                parent: Some(EntityId(bad.into())),
                ..NewEntity::new(child.clone(), "Contract Bad Parent", "contract-fixture")
            })
            .await
            .expect_err("a parent that is not a handle is malformed, not a candidate search");
        assert!(
            matches!(err, MemoryError::InvalidSubject(_)),
            "{bad:?} must be refused as a malformed id, got {err:?}"
        );
    }
    assert!(
        !store
            .list_entities(None)
            .await
            .expect("list_entities should succeed")
            .iter()
            .any(|e| e.id == child),
        "a refused write creates nothing"
    );
}

/// **A miss is a miss, not a leaf.** Asking for the children of something
/// that does not exist is an error carrying candidates — never an empty
/// list, which a caller would read as "this thing has nothing under it".
pub async fn children_of_an_unknown_entity_is_a_miss<M: Memory>(store: &M) {
    let known = EntityId("project:contract-ghost-parent".into());
    add(
        store,
        NewEntity::new(known.clone(), "Contract Ghost Parent", "contract-fixture"),
    )
    .await;

    let typo = EntityId("project:contract-ghost-parnt".into());
    let err = store
        .children(&typo)
        .await
        .expect_err("an unknown parent must not read as childless");
    let MemoryError::UnknownEntity { nearest, .. } = &err else {
        panic!("an unknown parent is an unknown entity, got {err:?}");
    };
    assert!(
        nearest.iter().any(|m| m.handle == known),
        "the miss names what it might have meant: {nearest:?}"
    );
}

/// **A parent must already exist.** Creating an entity under a handle
/// nothing resolves is blocked with candidates — and blocked means nothing
/// is written: not the child, and certainly not the parent it named.
/// Creation is an intentional act; naming a thing is not creating it.
pub async fn an_unnamed_parent_is_refused_and_provisions_nothing<M: Memory>(store: &M) {
    let real = EntityId("project:contract-plant".into());
    let typo = EntityId("project:contract-plnt".into());
    // Named so it resembles neither the real parent nor the typo: this
    // case is about the PARENT gate, and a child that tripped the screen on
    // its own handle first would report a block about itself.
    let child = EntityId("project:contract-shift-rota".into());
    add(
        store,
        NewEntity::new(real.clone(), "Contract Plant", "contract-fixture"),
    )
    .await;

    let blocked = store
        .add_entity(NewEntity {
            parent: Some(typo.clone()),
            ..NewEntity::new(child.clone(), "Shift Rota", "contract-fixture")
        })
        .await
        .expect("an unresolvable parent is an answer, not a failure");
    let Guarded::Blocked {
        attempted,
        candidates,
    } = blocked
    else {
        panic!("a parent that does not exist must block");
    };
    assert_eq!(
        attempted, typo,
        "the block is about the parent, so that is the handle it reports"
    );
    assert!(
        candidates.iter().any(|c| c.handle == real),
        "the answer names what it might have meant: {candidates:?}"
    );

    // **The token that clears a name collision must not conjure a parent.**
    // This is the token for exactly the refusal above — correctly derived,
    // not invented — and this gate still refuses it, because a write that
    // NAMES an entity has no override to offer.
    let held = guard::override_token(&attempted, &candidates);
    assert!(
        matches!(
            store
                .add_entity(NewEntity {
                    parent: Some(typo.clone()),
                    override_token: Some(held),
                    ..NewEntity::new(child.clone(), "Shift Rota", "contract-fixture")
                })
                .await
                .expect("an unresolvable parent is an answer, not a failure"),
            Guarded::Blocked { .. }
        ),
        "no token creates a parent: naming a thing is not creating it"
    );

    let known = store
        .list_entities(None)
        .await
        .expect("list_entities should succeed");
    assert!(
        !known.iter().any(|e| e.id == child || e.id == typo),
        "a blocked write creates neither the child nor the parent it named"
    );
}

/// **Nothing is its own parent.** Refused in the house shape — a blocked
/// result naming the offender, not a bare error — and never overridable,
/// because there is no honest "I checked, they're different" answer when
/// both handles are the same one.
pub async fn nothing_may_be_its_own_parent<M: Memory>(store: &M) {
    let ouroboros = EntityId("project:contract-ouroboros".into());
    let blocked = store
        .add_entity(NewEntity {
            parent: Some(ouroboros.clone()),
            ..NewEntity::new(ouroboros.clone(), "Contract Ouroboros", "contract-fixture")
        })
        .await
        .expect("a self-parenting write is an answer, not a failure");
    let Guarded::Blocked {
        attempted,
        candidates,
    } = blocked
    else {
        panic!("an entity naming itself as its parent must block");
    };
    assert!(
        candidates
            .iter()
            .any(|c| c.handle == ouroboros && c.reason == guard::MatchReason::SelfParent),
        "the answer says WHICH refusal this is, or it reads as an unknown handle: {candidates:?}"
    );

    // The token derived from this very refusal, handed back. It changes
    // nothing, because there is no honest "I checked, they're different"
    // answer when both handles are the same one.
    let held = guard::override_token(&attempted, &candidates);
    assert!(
        matches!(
            store
                .add_entity(NewEntity {
                    parent: Some(ouroboros.clone()),
                    override_token: Some(held),
                    ..NewEntity::new(ouroboros.clone(), "Contract Ouroboros", "contract-fixture")
                })
                .await
                .expect("a self-parenting write is an answer, not a failure"),
            Guarded::Blocked { .. }
        ),
        "self-parenting is never overridable"
    );
    assert!(
        !store
            .list_entities(None)
            .await
            .expect("list_entities should succeed")
            .iter()
            .any(|e| e.id == ouroboros),
        "a blocked write writes nothing at all"
    );
}

pub async fn prose_is_replaced_whole_and_reads_back<M: Memory>(store: &M) {
    let bot = EntityId("bot:contract-epsilon".into());
    add(
        store,
        NewEntity::new(bot.clone(), "Contract Epsilon", "contract-fixture"),
    )
    .await;
    let fact = capture(
        store,
        NewFact::about(bot.clone(), "answers before noon", date(2026, 7, 25)),
    )
    .await;

    let charter = "Keeps the schedule.\n\nHard line: never writes to the ledger.";
    let stored = store.set_prose(&bot, charter).await.expect("set_prose ok");
    assert_eq!(stored, charter, "the verb returns what a read will return");
    let scanned = store
        .scan_entity(&bot)
        .await
        .expect("scan_entity ok")
        .expect("an entity that exists has a doc");
    assert_eq!(scanned.prose, charter, "the read path returns it");

    // Replaced, never appended: a charter is what is so now, not a trail.
    let rewritten = "Keeps the schedule. Nothing else.";
    store
        .set_prose(&bot, rewritten)
        .await
        .expect("set_prose ok");
    let scanned = store
        .scan_entity(&bot)
        .await
        .expect("scan_entity ok")
        .expect("an entity that exists has a doc");
    assert_eq!(scanned.prose, rewritten);
    assert!(
        !scanned.prose.contains("ledger"),
        "the old prose is gone, not buried under the new: {}",
        scanned.prose
    );

    // …and the facts sharing the page are untouched by any of it.
    let facts = store.recall(&bot).await.expect("recall ok");
    assert!(
        facts
            .iter()
            .any(|f| f.id == fact.id && f.content == "answers before noon"),
        "a prose write must not disturb the facts beside it: {facts:?}"
    );

    // An empty charter is not a charter.
    assert!(
        store.set_prose(&bot, "   ").await.is_err(),
        "blank prose is refused rather than silently clearing the page"
    );

    // **And prose that would forge a document's own structure is refused by
    // EVERY store, not only the one that would be corrupted by it.** A
    // charter carrying the fact-table header moves the boundary between
    // prose and facts, and every fact below it stops being read as a fact.
    // Held here rather than in one adapter's own tests because a fake that
    // waves it through is how a green suite ships a store-corrupting write.
    for forged in [
        format!("a charter\n\n{FACTS_HEADER}\n\n| id | subject |"),
        FACTS_HEADER.to_string(),
        format!("   {FACTS_HEADER}   "),
    ] {
        let err = store
            .set_prose(&bot, &forged)
            .await
            .expect_err("prose carrying a reserved line must be refused");
        assert!(
            matches!(err, MemoryError::InvalidEntity(_)),
            "expected a refusal naming the prose, got {err:?} for {forged:?}"
        );
    }
    // …and the charter that was there is untouched by any refusal.
    let scanned = store
        .scan_entity(&bot)
        .await
        .expect("scan_entity ok")
        .expect("an entity that exists has a doc");
    assert_eq!(scanned.prose, rewritten, "a refused write changes nothing");

    // The words on their own, not on a line of their own, are just words.
    let mentioning = "the facts table is at the bottom of this page";
    assert_eq!(
        store
            .set_prose(&bot, mentioning)
            .await
            .expect("ordinary prose"),
        mentioning,
        "a sentence that merely mentions facts is prose"
    );

    // And a handle that names nothing is a miss — never a doc conjured to
    // hold the prose, the same rule every other verb here follows.
    let ghost = EntityId("bot:contract-ghost-bot".into());
    let err = store
        .set_prose(&ghost, "a charter for nobody")
        .await
        .expect_err("an unknown entity must miss");
    assert!(
        matches!(err, MemoryError::UnknownEntity { .. }),
        "expected an entity miss, got {err:?}"
    );
    assert!(
        !store
            .list_entities(None)
            .await
            .expect("list")
            .iter()
            .any(|e| e.id == ghost),
        "nothing was created along the way"
    );
}

/// `list_entities(kind)` narrows to one kind and never leaks another's.
pub async fn list_entities_filters_by_kind<M: Memory>(store: &M) {
    let place = EntityId("place:contract-north-trail".into());
    let topic = EntityId("topic:contract-widgets".into());
    add(
        store,
        NewEntity::new(place.clone(), "North Trail", "user-named"),
    )
    .await;
    add(
        store,
        NewEntity::new(topic.clone(), "Widgets", "user-named"),
    )
    .await;

    let places = store
        .list_entities(Some(EntityKind::PLACE))
        .await
        .expect("list places");
    assert!(places.iter().all(|e| e.kind == EntityKind::PLACE));
    assert!(places.iter().any(|e| e.id == place));
    assert!(
        !places.iter().any(|e| e.id == topic),
        "a topic must not appear in the place listing"
    );
}

/// An entity's metadata edits in place; the handle is untouched.
pub async fn update_entity_edits_metadata_in_place<M: Memory>(store: &M) {
    let id = EntityId("thing:contract-red-bike".into());
    add(store, NewEntity::new(id.clone(), "Red Bike", "user-named")).await;

    let updated = store
        .update_entity(
            &id,
            EntityPatch {
                name: Some("Red Bike (the gravel one)".into()),
                crm: Some("card:551".into()),
                ..Default::default()
            },
        )
        .await
        .expect("update_entity should succeed")
        .written()
        .expect("the guard must not block an uncontested rename");
    assert_eq!(updated.id, id, "the handle is immutable");
    assert_eq!(
        updated.source, "user-named",
        "an omitted field is left alone"
    );

    let seen = read_entity(store, &id).await;
    assert_eq!(seen.name, "Red Bike (the gravel one)");
    assert_eq!(seen.crm.as_deref(), Some("card:551"));
}

/// Renaming an entity onto a name the index already holds is screened by the
/// same guard that screens creation. Otherwise the guard is trivially
/// side-steppable: create under a throwaway name, then rename onto the
/// collision — and two people wear one name with no confirmation asked.
pub async fn update_entity_screens_a_colliding_rename<M: Memory>(store: &M) {
    let first = EntityId::person("person:contract-renamed-onto");
    let second = EntityId::person("person:contract-renamer");
    add(
        store,
        NewEntity::new(first.clone(), "Renamed Onto", "user-named"),
    )
    .await;
    add(
        store,
        NewEntity::new(second.clone(), "Renamer", "user-named"),
    )
    .await;

    let outcome = store
        .update_entity(
            &second,
            EntityPatch {
                name: Some("Renamed Onto".into()),
                ..Default::default()
            },
        )
        .await
        .expect("the call itself succeeds; the guard answers in the result");
    let Guarded::Blocked {
        attempted,
        candidates,
    } = outcome
    else {
        panic!("a rename onto an existing name must be blocked");
    };
    assert!(
        candidates.iter().any(|m| m.handle == first),
        "the guard must name the entity already wearing it: {candidates:?}"
    );
    let token = guard::override_token(&attempted, &candidates);

    let entities = store.list_entities(None).await.expect("list");
    let wearing_the_name: Vec<&EntityId> = entities
        .iter()
        .filter(|e| e.name == "Renamed Onto")
        .map(|e| &e.id)
        .collect();
    assert_eq!(
        wearing_the_name,
        vec![&first],
        "an unconfirmed rename onto an existing name must not land"
    );

    // A token nobody minted must not resolve it, or the mechanism is a
    // boolean with more ceremony.
    assert!(
        matches!(
            store
                .update_entity(
                    &second,
                    EntityPatch {
                        name: Some("Renamed Onto".into()),
                        override_token: Some("0000000000000000".into()),
                        ..Default::default()
                    },
                )
                .await
                .expect("the call itself succeeds; the guard answers in the result"),
            Guarded::Blocked { .. }
        ),
        "a token this refusal did not mint resolves nothing"
    );

    // The token this refusal minted clears a rename, exactly as it clears a
    // creation — one mechanism over both gates.
    let forced = store
        .update_entity(
            &second,
            EntityPatch {
                name: Some("Renamed Onto".into()),
                override_token: Some(token),
                ..Default::default()
            },
        )
        .await
        .expect("update should succeed")
        .written()
        .expect("the refusal's own token resolves the rename");
    assert_eq!(forced.name, "Renamed Onto");
    assert_eq!(forced.id, second, "the handle is untouched by a rename");
}

/// A rename is screened on the **name** channel only. An entity whose handle
/// is a near-slug of another's — a collision already adjudicated when it was
/// created — must still be freely renamable: re-screening the immutable
/// handle turned that one decision into a permanent block on the name field.
pub async fn update_entity_does_not_re_screen_the_handle<M: Memory>(store: &M) {
    let settled = EntityId::person("person:contract-nearslug");
    let neighbour = EntityId::person("person:contract-nearslugg");
    add(store, NewEntity::new(settled, "Nearslug One", "user-named")).await;
    // The near-slug the guard reported, judged different at creation — made
    // the way a caller really makes one, over the refusal's own token.
    add_over_the_screen(
        store,
        NewEntity::new(neighbour.clone(), "Quite Another Two", "user-named"),
    )
    .await;

    let renamed = store
        .update_entity(
            &neighbour,
            EntityPatch {
                name: Some("Quite Another Three".into()),
                ..Default::default()
            },
        )
        .await
        .expect("update_entity should succeed")
        .written()
        .expect("a near-slug settled at creation must not block a later name edit");
    assert_eq!(renamed.name, "Quite Another Three");
}

/// Editing metadata that isn't the name is never screened — an entity's own
/// name must not trip the guard against itself.
///
/// An alias is a name: claiming one that another entity already answers
/// to is the same collision a rename is, so it must face the same gate,
/// even on a patch that renames nothing — otherwise search would index
/// two entities answering to one word.
pub async fn update_entity_screens_a_colliding_alias<M: Memory>(store: &M) {
    let owner = EntityId::person("person:contract-alias-owner");
    add(
        store,
        NewEntity::new(owner.clone(), "Contract Alias Owner", "user-named"),
    )
    .await;
    let borrower = EntityId::person("person:contract-alias-borrower");
    add(
        store,
        NewEntity::new(borrower.clone(), "Contract Alias Borrower", "user-named"),
    )
    .await;

    let outcome = store
        .update_entity(
            &borrower,
            EntityPatch {
                aliases: Some(vec!["Contract Alias Owner".into()]),
                ..Default::default()
            },
        )
        .await
        .expect("the call itself succeeds; the guard answers in the result");
    let Guarded::Blocked {
        attempted,
        candidates,
    } = outcome
    else {
        panic!("an alias onto a name another entity wears must be blocked");
    };
    assert!(
        candidates.iter().any(|m| m.handle == owner),
        "the guard must name the entity already wearing it: {candidates:?}"
    );
    let token = guard::override_token(&attempted, &candidates);
    assert!(
        store
            .list_entities(None)
            .await
            .expect("list")
            .iter()
            .filter(|e| e.id == borrower)
            .all(|e| e.aliases.is_empty()),
        "a blocked alias write lands nothing"
    );

    // The same mechanism that clears a rename clears this: names are not
    // unique, and two entities may legitimately answer to one word.
    let forced = store
        .update_entity(
            &borrower,
            EntityPatch {
                aliases: Some(vec!["Contract Alias Owner".into()]),
                override_token: Some(token),
                ..Default::default()
            },
        )
        .await
        .expect("update should succeed")
        .written()
        .expect("the refusal's own token resolves the collision");
    assert_eq!(forced.aliases, vec!["Contract Alias Owner".to_string()]);
}

/// An alias the entity **already wears** is not a new claim, so re-sending it
/// is not a collision with itself. Without this, every later patch to an
/// entity's alias set comes back blocked by the entity's own name.
pub async fn update_entity_is_not_blocked_by_its_own_labels<M: Memory>(store: &M) {
    let id = EntityId("org:contract-self-labelled".into());
    add(
        store,
        NewEntity {
            aliases: vec!["Contract Self Nickname".into()],
            ..NewEntity::new(id.clone(), "Contract Self-Labelled", "user-named")
        },
    )
    .await;

    let again = store
        .update_entity(
            &id,
            EntityPatch {
                name: Some("Contract Self-Labelled".into()),
                aliases: Some(vec!["Contract Self Nickname".into()]),
                crm: Some("card:552".into()),
                ..Default::default()
            },
        )
        .await
        .expect("update should succeed")
        .written()
        .expect("an entity is never a candidate for its own labels");
    assert_eq!(again.crm.as_deref(), Some("card:552"));
    assert_eq!(again.aliases, vec!["Contract Self Nickname".to_string()]);
}

/// **Only a change of LABEL is screened.** source, crm and boot say nothing
/// about what a thing is called, so they can introduce no collision — and a
/// gate that fired on them would make an already-settled duplicate name
/// permanently uneditable in every other field.
pub async fn update_entity_without_a_rename_is_not_screened<M: Memory>(store: &M) {
    let first = EntityId("org:contract-unscreened".into());
    add(
        store,
        NewEntity::new(first.clone(), "Unscreened Org", "user-named"),
    )
    .await;
    // A second entity that legitimately shares the name — settled once, at
    // creation, over that refusal's own token. That settlement must not be
    // re-litigated by a patch that touches no label at all.
    let twin = EntityId("org:contract-unscreened-twin".into());
    add_over_the_screen(
        store,
        NewEntity::new(twin.clone(), "Unscreened Org", "user-named"),
    )
    .await;

    let metadata_only = store
        .update_entity(
            &twin,
            EntityPatch {
                source: Some("crm-card".into()),
                ..Default::default()
            },
        )
        .await
        .expect("update should succeed")
        .written()
        .expect("a patch that renames nothing is not a rename");
    assert_eq!(metadata_only.source, "crm-card");
    assert_eq!(
        metadata_only.name, "Unscreened Org",
        "and it left the name alone"
    );
}

/// Updating an entity that doesn't exist errors with the nearest candidates
/// — it never quietly creates one.
pub async fn update_entity_unknown_handle_never_creates<M: Memory>(store: &M) {
    let ghost = EntityId("thing:contract-red-bikee".into());
    let err = store
        .update_entity(
            &ghost,
            EntityPatch {
                name: Some("nope".into()),
                ..Default::default()
            },
        )
        .await
        .expect_err("an unknown handle must error");
    let MemoryError::UnknownEntity { nearest, .. } = &err else {
        panic!("expected UnknownEntity, got {err:?}");
    };
    assert!(
        nearest
            .iter()
            .any(|m| m.handle.slug() == "contract-red-bike"),
        "the error must name the near miss: {nearest:?}"
    );
    assert!(
        store
            .list_entities(None)
            .await
            .expect("list")
            .iter()
            .all(|e| e.id != ghost),
        "a failed update must not have created the entity"
    );
}

// --- addresses and update ------------------------------------------------

/// Every fact read back carries its global address, and that address is
/// exactly what `update_fact` accepts — the pairing that makes facts
/// editable at all.
pub async fn facts_carry_a_usable_address<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-addressable");
    let captured = capture(
        store,
        NewFact::about(subject.clone(), "address me", date(2026, 7, 24)),
    )
    .await;
    let seen = read_back(store, &subject, &captured.id).await;
    let address = seen.address();
    assert_eq!(
        address.home, subject,
        "a fact's home is the doc it lives in"
    );
    assert_eq!(
        FactAddress::parse(&address.to_string()).expect("the address must round-trip"),
        address
    );

    let updated = edit(
        store,
        &address,
        FactPatch {
            content: Some("addressed and edited".into()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(updated.content, "addressed and edited");
}

/// An edit rewrites the row in place — fix-the-source — and the read path
/// shows the new truth with no second copy left beside it.
pub async fn update_fact_edits_in_place<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-editable");
    let captured = capture(
        store,
        NewFact::about(subject.clone(), "works at the old place", date(2026, 7, 1)),
    )
    .await;
    edit(
        store,
        &captured.address(),
        FactPatch {
            content: Some("works at the new place".into()),
            details: Some("changed jobs in July".into()),
            ..Default::default()
        },
    )
    .await;

    let facts = store.recall(&subject).await.expect("recall");
    let seen = read_back(store, &subject, &captured.id).await;
    assert_eq!(seen.content, "works at the new place");
    assert_eq!(seen.details.as_deref(), Some("changed jobs in July"));
    assert_eq!(
        facts.iter().filter(|f| f.id == captured.id).count(),
        1,
        "an edit rewrites the row; it never appends a second one"
    );
    assert!(
        !facts.iter().any(|f| f.content == "works at the old place"),
        "the old claim must be gone, not left beside the new one"
    );
}

/// **An edit can carry a new day, and one that names none leaves the
/// record's day alone.**
///
/// A record rewritten later keeps the day of the claim it replaces unless
/// the caller names a new one — otherwise a correction made months after
/// the original claim reads back as if it were true on the original day
/// forever, with no way for a caller to say when the correction itself
/// happened.
///
/// **Both halves.** A day given is the day carried, and no day given is
/// still the day the claim was captured under — without the second, a
/// build that always overwrote the day with the one supplied (or that
/// silently dropped the field) would satisfy the first alone.
pub async fn an_edit_can_carry_a_new_day_and_omitted_leaves_it_alone<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-redated");
    let captured = capture(
        store,
        NewFact::about(
            subject.clone(),
            "the club meets on Tuesdays",
            date(2026, 7, 1),
        ),
    )
    .await;
    assert_eq!(captured.recorded_at, date(2026, 7, 1));

    let redated = edit(
        store,
        &captured.address(),
        FactPatch {
            content: Some("the club meets on Wednesdays".into()),
            recorded_at: Some(date(2026, 8, 15)),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        redated.recorded_at,
        date(2026, 8, 15),
        "a correction given a day must carry that day, not the day of the claim it replaces"
    );

    let untouched = edit(
        store,
        &redated.address(),
        FactPatch {
            content: Some("the club meets on Thursdays".into()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        untouched.recorded_at,
        date(2026, 8, 15),
        "an edit naming no day must leave the record's existing day alone"
    );
}

/// **A refutation is an ordinary content edit**, not a status. It rewrites
/// the row in place to state the negative truth, keeps its id, and stays
/// `active` — because "does NOT play the theremin" IS the current truth
/// about this entity, and the reader must find it on a plain default read.
///
/// The alternative — a `negated` flag beside the disproved claim — is the
/// "was wrong, see flag" shape: it leaves two versions on the page for the
/// reader to adjudicate, and hides the correction from every default
/// search, which is precisely where it is needed.
pub async fn a_refutation_is_an_ordinary_content_edit<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-refutable");
    let captured = capture(
        store,
        NewFact::about(
            subject.clone(),
            "a close contact of the user",
            date(2026, 7, 1),
        ),
    )
    .await;
    let refuted = edit(
        store,
        &captured.address(),
        FactPatch {
            content: Some("NOT a close contact — do not re-infer closeness".into()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        refuted.id, captured.id,
        "the row is rewritten, not replaced"
    );

    let seen = read_back(store, &subject, &captured.id).await;
    assert_eq!(
        seen.status,
        FactStatus::Active,
        "the negative truth is the truth"
    );
    assert!(seen.content.starts_with("NOT a close contact"));

    let facts = store.recall(&subject).await.expect("recall");
    assert!(
        !facts
            .iter()
            .any(|f| f.content == "a close contact of the user"),
        "the refuted claim is gone from the page, not flagged beside it: {facts:?}"
    );
}

/// Promotion to testimony is gated on the user's explicit confirmation —
/// and a refused promotion leaves the fact exactly as it was.
pub async fn promotion_to_testimony_needs_confirmation<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-promotable");
    let captured = capture(
        store,
        NewFact::about(subject.clone(), "prefers mornings", date(2026, 7, 1)),
    )
    .await;
    assert_eq!(captured.provenance, Provenance::Inference);

    let err = store
        .update_fact(
            &captured.address(),
            FactPatch {
                provenance: Some(Provenance::Testimony),
                ..Default::default()
            },
        )
        .await
        .expect_err("an unconfirmed promotion must be refused");
    assert!(
        matches!(err, MemoryError::UnconfirmedPromotion),
        "expected UnconfirmedPromotion, got {err:?}"
    );
    assert_eq!(
        read_back(store, &subject, &captured.id).await.provenance,
        Provenance::Inference,
        "a refused promotion must leave the fact untouched"
    );

    let promoted = edit(
        store,
        &captured.address(),
        FactPatch {
            provenance: Some(Provenance::Testimony),
            confirmed_by_user: true,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(promoted.provenance, Provenance::Testimony);
    assert_eq!(
        read_back(store, &subject, &captured.id).await.provenance,
        Provenance::Testimony
    );
}

/// **A hedge round-trips as itself.** The claim the second field exists
/// for: the operator says something and says they are not sure of it, so
/// `testimony` (they back it) and `open` (they are not sure) are both true
/// and both stored. One field for the two questions makes a session pick
/// which of them to be wrong about.
pub async fn a_hedged_claim_round_trips<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-hedged-word");
    let captured = capture(
        store,
        NewFact {
            provenance: Provenance::Testimony,
            standing: Some(Standing::Open),
            ..NewFact::about(
                subject.clone(),
                "thinks the shop shuts early",
                date(2026, 7, 1),
            )
        },
    )
    .await;
    assert_eq!(captured.provenance, Provenance::Testimony);
    assert_eq!(captured.standing, Standing::Open);
    assert_eq!(read_back(store, &subject, &captured.id).await, captured);
}

/// **A silent standing follows the provenance**, which is what every claim
/// written before this field meant — so only a hedge has to be asked for,
/// and no existing row changed meaning when the column arrived.
pub async fn standing_defaults_to_what_the_provenance_implies<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-silent-standing");
    let said = capture(
        store,
        NewFact {
            provenance: Provenance::Testimony,
            ..NewFact::about(subject.clone(), "opens at seven", date(2026, 7, 1))
        },
    )
    .await;
    assert_eq!(
        said.standing,
        Standing::Settled,
        "the operator's word is settled unless they hedge it"
    );

    let guessed = capture(
        store,
        NewFact::about(
            subject.clone(),
            "probably busy on Fridays",
            date(2026, 7, 2),
        ),
    )
    .await;
    assert_eq!(
        guessed.standing,
        Standing::Open,
        "a claim nobody confirmed is open"
    );

    // Both survive storage, not just the values the domain computed.
    assert_eq!(
        read_back(store, &subject, &said.id).await.standing,
        Standing::Settled
    );
    assert_eq!(
        read_back(store, &subject, &guessed.id).await.standing,
        Standing::Open
    );
}

/// **A capture declares its own standing**, on honour, exactly as it
/// declares its provenance. A derived claim the operator has since
/// confirmed is an ordinary row: the axes are independent, and the gate is
/// on settling an open claim rather than on the pairing.
pub async fn a_capture_declares_its_own_standing<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-unbacked-guess");
    ensure(store, &subject).await;
    let settled = capture(
        store,
        NewFact {
            provenance: Provenance::Inference,
            standing: Some(Standing::Settled),
            ..NewFact::about(subject.clone(), "certainly shuts at nine", date(2026, 7, 1))
        },
    )
    .await;
    assert_eq!(settled.provenance, Provenance::Inference);
    assert_eq!(settled.standing, Standing::Settled);

    // Paired with the default it did not take: silence still resolves from
    // the provenance, so declaring a standing is a choice rather than the
    // only way to get one.
    let open = capture(
        store,
        NewFact::about(subject.clone(), "certainly shuts at nine", date(2026, 7, 1)),
    )
    .await;
    assert_eq!(open.standing, Standing::Open);
}

/// **Settling is the user's move, and it leaves provenance alone.**
///
/// A hedge the operator wrote is testimony from the start, so there is no
/// provenance for a confirmation to promote. What confirmation closes is
/// the standing, and there must still be something for it to close. A
/// promotion that "works" here is one reading a hedge stored as inference,
/// which is a claim mis-stored rather than a claim settled.
pub async fn settling_a_hedge_needs_confirmation_and_keeps_its_provenance<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-settle-gate");
    let hedged = capture(
        store,
        NewFact {
            provenance: Provenance::Testimony,
            standing: Some(Standing::Open),
            ..NewFact::about(subject.clone(), "thinks it shuts early", date(2026, 7, 1))
        },
    )
    .await;

    let err = store
        .update_fact(
            &hedged.address(),
            FactPatch {
                standing: Some(Standing::Settled),
                ..Default::default()
            },
        )
        .await
        .expect_err("an unconfirmed settling must be refused");
    assert!(
        matches!(err, MemoryError::UnconfirmedSettling),
        "expected UnconfirmedSettling, got {err:?}"
    );
    assert_eq!(
        read_back(store, &subject, &hedged.id).await.standing,
        Standing::Open,
        "a refused settling must leave the claim untouched"
    );

    let settled = edit(
        store,
        &hedged.address(),
        FactPatch {
            standing: Some(Standing::Settled),
            confirmed_by_user: true,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(settled.standing, Standing::Settled);
    assert_eq!(
        settled.provenance,
        Provenance::Testimony,
        "confirmation closes the hedge; it does not restate who backed the claim"
    );
    let seen = read_back(store, &subject, &hedged.id).await;
    assert_eq!(seen.standing, Standing::Settled);
    assert_eq!(seen.provenance, Provenance::Testimony);
}

/// **Reopening is free**, exactly as demotion to inference is free. Nothing
/// is risked by a claim admitting it might be wrong.
pub async fn reopening_a_settled_claim_needs_no_ceremony<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-reopening");
    let settled = capture(
        store,
        NewFact {
            provenance: Provenance::Testimony,
            ..NewFact::about(subject.clone(), "opens at seven", date(2026, 7, 1))
        },
    )
    .await;
    assert_eq!(settled.standing, Standing::Settled);

    let reopened = edit(
        store,
        &settled.address(),
        FactPatch {
            standing: Some(Standing::Open),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(reopened.standing, Standing::Open);
    assert_eq!(
        read_back(store, &subject, &settled.id).await.standing,
        Standing::Open
    );
}

/// **One axis moves at a time.** A patch names what it changes: promoting a
/// guess says who backs it now and nothing about how sure anyone is, so a
/// caller who means both says both. The alternative is the coupling the
/// second field exists to remove — move one and the other follows.
pub async fn a_patch_moves_only_the_axis_it_names<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-confirmed-guess");
    let guess = capture(
        store,
        NewFact::about(subject.clone(), "probably shuts at nine", date(2026, 7, 1)),
    )
    .await;
    assert_eq!(guess.provenance, Provenance::Inference);
    assert_eq!(guess.standing, Standing::Open);

    let promoted = edit(
        store,
        &guess.address(),
        FactPatch {
            provenance: Some(Provenance::Testimony),
            confirmed_by_user: true,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(promoted.provenance, Provenance::Testimony);
    assert_eq!(
        promoted.standing,
        Standing::Open,
        "a promotion that said nothing about standing moved it anyway"
    );
    assert_eq!(
        read_back(store, &subject, &guess.id).await.standing,
        Standing::Open,
        "…and it reached the store that way"
    );

    // Paired with the positive: naming both is what lands both, so the
    // assertion above is about the silence rather than about a claim that
    // cannot be settled at all.
    let settled = edit(
        store,
        &guess.address(),
        FactPatch {
            standing: Some(Standing::Settled),
            confirmed_by_user: true,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(settled.provenance, Provenance::Testimony);
    assert_eq!(settled.standing, Standing::Settled);

    // And the reverse silence too: demoting says nothing about standing.
    let demoted = edit(
        store,
        &guess.address(),
        FactPatch {
            provenance: Some(Provenance::Inference),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(demoted.provenance, Provenance::Inference);
    assert_eq!(demoted.standing, Standing::Settled);
}

/// Demotion needs no ceremony — only promotion is gated.
pub async fn demotion_to_inference_is_free<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-demotable");
    let captured = capture(
        store,
        NewFact {
            provenance: Provenance::Testimony,
            ..NewFact::about(subject.clone(), "said to like winter", date(2026, 7, 1))
        },
    )
    .await;
    let demoted = edit(
        store,
        &captured.address(),
        FactPatch {
            provenance: Some(Provenance::Inference),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(demoted.provenance, Provenance::Inference);
}

/// An unknown address errors with the addresses that do exist, and writes
/// nothing — the never-guess rule on the update path.
pub async fn update_fact_unknown_address_never_creates<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-missing-row");
    let captured = capture(
        store,
        NewFact::about(subject.clone(), "the only row here", date(2026, 7, 1)),
    )
    .await;
    let ghost = FactAddress::new(subject.clone(), FactId("f999".into()));
    let err = store
        .update_fact(
            &ghost,
            FactPatch {
                content: Some("nope".into()),
                ..Default::default()
            },
        )
        .await
        .expect_err("an unknown address must error");
    let MemoryError::UnknownFact { nearest, .. } = &err else {
        panic!("expected UnknownFact, got {err:?}");
    };
    assert!(
        nearest.contains(&captured.address().to_string()),
        "the error must list the addresses that do exist: {nearest:?}"
    );

    let facts = store.recall(&subject).await.expect("recall");
    assert_eq!(facts.len(), 1, "nothing was created: {facts:?}");
    assert_eq!(facts[0].content, "the only row here");
}

/// **An address that misses on its HANDLE is an entity miss, not a fact
/// miss.** It came back as "no fact at 'person:zenit#f1'; addresses here:"
/// — a dangling empty list that named nothing and pointed at nothing, while
/// the actual mistake was one field to the left. The two misses have
/// different causes and different fixes, so they say different things: an
/// unknown handle answers with near misses, exactly as `update_entity`
/// does, and a known entity that simply holds no rows says so plainly.
pub async fn update_fact_tells_an_unknown_handle_from_an_empty_entity<M: Memory>(store: &M) {
    let known = EntityId::person("person:contract-addressee");
    add(
        store,
        NewEntity::new(known.clone(), "Addressee", "user-named"),
    )
    .await;

    let nudge = || FactPatch {
        content: Some("nope".into()),
        ..Default::default()
    };

    let typo = EntityId::person("person:contract-addresse");
    let err = store
        .update_fact(&FactAddress::new(typo, FactId("f1".into())), nudge())
        .await
        .expect_err("an address on an unknown handle must error");
    let MemoryError::UnknownEntity { nearest, .. } = &err else {
        panic!("a handle that names no entity is an entity miss, got {err:?}");
    };
    assert!(
        nearest.iter().any(|m| m.handle == known),
        "…and it names the near miss the caller probably meant: {nearest:?}"
    );

    // The entity is real; it just has nothing in it yet.
    let err = store
        .update_fact(
            &FactAddress::new(known.clone(), FactId("f1".into())),
            nudge(),
        )
        .await
        .expect_err("an address on an entity with no facts must error");
    let MemoryError::UnknownFact { nearest, .. } = &err else {
        panic!("a real entity with no rows is a fact miss, got {err:?}");
    };
    assert!(
        nearest.is_empty(),
        "there are no addresses to list: {nearest:?}"
    );
    assert!(
        !err.to_string().trim_end().ends_with(':'),
        "the message must not trail off into an empty list: {err}"
    );

    // …and once it holds one, the miss lists what does exist.
    let real = capture(
        store,
        NewFact::about(known.clone(), "the only row here", date(2026, 7, 1)),
    )
    .await;
    let err = store
        .update_fact(&FactAddress::new(known, FactId("f999".into())), nudge())
        .await
        .expect_err("an unknown row must still error");
    let MemoryError::UnknownFact { nearest, .. } = &err else {
        panic!("expected UnknownFact, got {err:?}");
    };
    assert!(
        nearest.contains(&real.address().to_string()),
        "got {nearest:?}"
    );
}

// --- structured edges at capture -----------------------------------------

/// An edge is written atomically with its fact and comes back on the read
/// path. This is what makes ask-across an edge walk instead of an AI reading
/// prose, so it is bound by the same read-back invariant as the row itself.
pub async fn capture_writes_an_edge_that_reads_back<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-edged");
    let edge = Edge::new(
        EdgeShape::Location,
        EntityId("place:contract-far-country".into()),
    );
    let captured = capture(
        store,
        NewFact {
            edge: Some(edge.clone()),
            ..NewFact::about(
                subject.clone(),
                "spending the winter away",
                date(2026, 7, 1),
            )
        },
    )
    .await;
    assert_eq!(captured.edge.as_ref(), Some(&edge));

    let seen = read_back(store, &subject, &captured.id).await;
    assert_eq!(
        seen, captured,
        "the edge must survive read-back byte-identical"
    );
    assert_eq!(seen.edge.map(|e| e.object), Some(edge.object));
}

/// Every shape survives the trip, each with an object of the kind it requires.
pub async fn every_edge_shape_reads_back<M: Memory>(store: &M) {
    let shapes = [
        (EdgeShape::Location, EntityKind::PLACE),
        (EdgeShape::Membership, EntityKind::ORG),
        (EdgeShape::Attendance, EntityKind::EVENT),
        (EdgeShape::About, EntityKind::TOPIC),
    ];
    for (shape, kind) in shapes {
        let subject = EntityId::person(format!("contract-shape-{shape}"));
        let object = EntityId::new(kind, format!("contract-object-{shape}"));
        let captured = capture(
            store,
            NewFact {
                edge: Some(Edge::new(shape, object.clone())),
                ..NewFact::about(
                    subject.clone(),
                    format!("a {shape} claim"),
                    date(2026, 7, 1),
                )
            },
        )
        .await;
        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(
            seen.edge,
            Some(Edge::new(shape, object)),
            "the {shape} edge must read back"
        );
    }
}

/// Nothing was recorded for `subject`: either its page reads empty or the
/// entity never came to exist at all — both prove a refused write did not
/// land. (Recalling a nonexistent entity is a miss by contract, so a spec
/// probing a subject it never ensured accepts the miss as its proof.)
async fn assert_nothing_recorded<M: Memory>(store: &M, subject: &EntityId) {
    match store.recall(subject).await {
        Ok(facts) => assert!(facts.is_empty(), "nothing must be written: {facts:?}"),
        Err(MemoryError::UnknownEntity { .. }) => {}
        Err(other) => panic!("unexpected recall error: {other:?}"),
    }
}

/// An object of the wrong kind for its shape is refused outright, and the
/// fact does not land either — the edge is part of the write, not a garnish.
pub async fn a_wrong_kind_edge_object_is_refused<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-miskinded");
    let err = store
        .capture(NewFact {
            // A `location` must point at a place; this one points at a person.
            edge: Some(Edge::new(
                EdgeShape::Location,
                EntityId::person("person:contract-alpha"),
            )),
            ..NewFact::about(subject.clone(), "should never be stored", date(2026, 7, 1))
        })
        .await
        .expect_err("a wrong-kind edge object must be refused");
    assert!(matches!(err, MemoryError::InvalidEdge(_)), "got {err:?}");
    assert_nothing_recorded(store, &subject).await;
}

/// The **object is screened by the write guard exactly as a subject is.** A
/// typo'd object is where ask-across quietly rots: the edge points at a node
/// nobody else references, so the walk comes back empty and nothing looks
/// wrong. It comes back as candidates instead, and nothing is written.
pub async fn an_edge_object_is_screened_by_the_guard<M: Memory>(store: &M) {
    let object = EntityId("place:contract-riverbend".into());
    add(
        store,
        NewEntity::new(object.clone(), "Riverbend", "user-named"),
    )
    .await;

    let subject = EntityId::person("person:contract-edge-guarded");
    // The subject faces the gate too, so it is provisioned first: this spec
    // is about the object, and the guard reports the first handle it stops.
    add(
        store,
        NewEntity::new(subject.clone(), "Edge Guarded", "user-named"),
    )
    .await;

    let typo = EntityId("place:contract-riverbnd".into());
    let outcome = store
        .capture(NewFact {
            edge: Some(Edge::new(EdgeShape::Location, typo.clone())),
            ..NewFact::about(subject.clone(), "should not land yet", date(2026, 7, 1))
        })
        .await
        .expect("the call itself succeeds; the guard answers in the result");
    let Guarded::Blocked {
        attempted,
        candidates,
    } = outcome
    else {
        panic!("a near-miss edge object must be reported");
    };
    assert_eq!(attempted, typo, "the guard names the handle it stopped");
    assert!(
        candidates.iter().any(|m| m.handle == object),
        "the guard must name the place it suspects: {candidates:?}"
    );
    assert_nothing_recorded(store, &subject).await;

    // Confirming the existing object is the ordinary path out.
    let landed = capture(
        store,
        NewFact {
            edge: Some(Edge::new(EdgeShape::Location, object.clone())),
            ..NewFact::about(subject.clone(), "now it lands", date(2026, 7, 1))
        },
    )
    .await;
    assert_eq!(landed.edge.map(|e| e.object), Some(object));
}

/// **`update_fact` sets and clears a field, and the store keeps both
/// moves.**
///
/// A contract case rather than a domain one, because the claim is about
/// STORAGE: the patch is applied to a record in memory either way, and only
/// a real store can say that a cleared key left the rows under the fact
/// rather than lingering there to be read back on the next call.
pub async fn update_fact_sets_and_clears_a_field<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-field-edit");
    let captured = capture(
        store,
        NewFact {
            fields: [
                ("cost".to_string(), "40".to_string()),
                ("done_on".to_string(), "2026-04-18".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(subject.clone(), "the annual service", date(2026, 7, 1))
        },
    )
    .await;

    let edited = edit(
        store,
        &captured.address(),
        FactPatch {
            fields: [("cost".to_string(), "45".to_string())]
                .into_iter()
                .collect(),
            clear_fields: vec!["done_on".to_string()],
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        edited.fields,
        [("cost".to_string(), "45".to_string())]
            .into_iter()
            .collect(),
        "the key named is rewritten and the key cleared is gone"
    );

    let seen = read_back(store, &subject, &captured.id).await;
    assert_eq!(
        seen.fields.get("cost").map(String::as_str),
        Some("45"),
        "the new value is on the read path"
    );
    assert!(
        !seen.fields.contains_key("done_on"),
        "…and the cleared key did not survive the store: {seen:?}"
    );
}

/// **A key written a hundred times holds one value and counts a hundred.**
///
/// The two questions the substrate exists to answer with one body of data.
/// Nobody wants the hundred sittings when they ask what the count is now;
/// one time in a hundred they want every one of them, with its date and the
/// record it arrived in.
///
/// **The address is the thing and the key.** Every sitting here is a record
/// of its own with an id of its own, so a history hanging off a record's id
/// would return a hundred histories of length one — which is the same
/// firehose the caller already had, and counts nothing.
pub async fn a_key_written_many_times_holds_one_value_and_counts<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-counted");
    let key = "donuts_eaten";
    let written = 100;
    for nth in 1..=written {
        capture(
            store,
            NewFact {
                fields: [(key.to_string(), nth.to_string())].into_iter().collect(),
                ..NewFact::about(subject.clone(), format!("ate one, number {nth}"), {
                    date(2026, 7, 1)
                })
            },
        )
        .await;
    }

    assert_eq!(
        store
            .fields(&subject)
            .await
            .expect("the fields of a thing should succeed")
            .get(key)
            .map(String::as_str),
        Some(written.to_string().as_str()),
        "the ordinary read is current truth: the newest write, and one value"
    );

    let history = store
        .history(&subject, key)
        .await
        .expect("history should succeed");
    assert_eq!(
        history.len(),
        written,
        "the count of the writes IS the answer to how many times"
    );
    assert_eq!(
        history.first().map(|w| w.value.as_deref()),
        Some(Some("1")),
        "oldest first: {:?}",
        history.first()
    );
    assert_eq!(
        history.last().map(|w| w.value.as_deref()),
        Some(Some(written.to_string().as_str())),
        "…and the newest last"
    );
    assert!(
        history
            .iter()
            .all(|w| w.recorded_at == date(2026, 7, 1) && w.status == FactStatus::Active),
        "every write says when it happened and what became of the record that carried it"
    );
    // Each write names the record it arrived in, and the addresses differ:
    // that is what makes the history worth having rather than a list of
    // values, and what proves the writes were not gathered off one row.
    let records: std::collections::BTreeSet<String> =
        history.iter().map(|w| w.fact.to_string()).collect();
    assert_eq!(
        records.len(),
        written,
        "each sitting is its own record and the history says which"
    );
}

/// **A key declared a counter reads back as the total of its writes, and
/// the writes are all still there.**
///
/// The declaration is what buys it, so this asserts the fold both ways in
/// one case: the counter adds, and an undeclared key written the same
/// number of times on the same thing still takes its newest write. A case
/// that only looked at the counter would pass on a store that sums every
/// number it holds.
///
/// It also reads the declaration back, because the fold travels through the
/// store's own type rows: a store that dropped the column would keep every
/// key on newest-wins and give no other sign of it.
pub async fn a_counter_totals_its_writes_and_keeps_them<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-totalled");
    store
        .declare_type(crate::memory::types::DeclaredType::new(
            "contract-snacking",
            vec![
                crate::memory::types::Field::summing("donuts").needed(),
                crate::memory::types::Field::required(
                    "mood",
                    crate::memory::types::ValueType::Text,
                ),
            ],
        ))
        .await
        .expect("declaring a type should succeed");
    assert_eq!(
        store
            .declared_types()
            .await
            .expect("the roster reads")
            .iter()
            .find(|t| t.name == "contract-snacking")
            .and_then(|t| t.field("donuts"))
            .map(|f| f.folds),
        Some(crate::memory::types::Fold::Sum),
        "the fold survives the store, or nothing below is about a counter"
    );

    for (nth, mood) in [(1, "hopeful"), (2, "content"), (3, "queasy")] {
        capture(
            store,
            NewFact {
                fields: [
                    ("donuts".to_string(), "1".to_string()),
                    ("mood".to_string(), mood.to_string()),
                ]
                .into_iter()
                .collect(),
                ..NewFact::about(
                    subject.clone(),
                    format!("a donut, number {nth}"),
                    date(2026, 7, 1),
                )
            },
        )
        .await;
    }

    let held = store
        .fields(&subject)
        .await
        .expect("the fields of a thing should succeed");
    assert_eq!(
        held.get("donuts").map(String::as_str),
        Some("3"),
        "three writes of one add up: {held:?}"
    );
    assert_eq!(
        held.get("mood").map(String::as_str),
        Some("queasy"),
        "and the key nobody declared a counter takes its newest write: {held:?}"
    );

    let history = store
        .history(&subject, "donuts")
        .await
        .expect("history should succeed");
    assert_eq!(
        history
            .iter()
            .map(|w| w.value.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("1"), Some("1"), Some("1")],
        "the projection adds them up and the substrate keeps each one: {history:?}"
    );
}

/// **An edit appends; the value it replaced stays readable.**
///
/// Fix-the-source is the surface — the record reads back changed, with no
/// second copy beside it — and underneath, the write that put the old value
/// there is still a write that happened. Both halves in one case: the
/// projection alone passes on a store that overwrites, and the history
/// alone passes on one that never projects.
pub async fn an_edit_appends_and_the_value_it_replaced_stays_in_the_history<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-appended");
    let captured = capture(
        store,
        NewFact {
            fields: [("cost".to_string(), "40".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(subject.clone(), "the annual service", date(2026, 4, 18))
        },
    )
    .await;
    edit(
        store,
        &captured.address(),
        FactPatch {
            fields: [("cost".to_string(), "45".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        },
    )
    .await;

    let seen = read_back(store, &subject, &captured.id).await;
    assert_eq!(
        seen.fields.get("cost").map(String::as_str),
        Some("45"),
        "the record reads back edited, which is the surface"
    );

    let history = store
        .history(&subject, "cost")
        .await
        .expect("history should succeed");
    assert_eq!(
        history
            .iter()
            .map(|w| w.value.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("40"), Some("45")],
        "both writes are there, oldest first: {history:?}"
    );
    assert!(
        history.iter().all(|w| w.fact == captured.address()),
        "an edit is a write on the record it edited"
    );
}

/// **Clearing a key is a write, not a removal.**
///
/// The key stops being current — the record reads back without it — and the
/// writes that put it there are still in its history, with the clear itself
/// recorded as the write that took it off. Nothing is deleted from the
/// substrate, which is the same one-way rule retraction runs on.
pub async fn clearing_a_key_leaves_its_writes_behind<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-cleared");
    let captured = capture(
        store,
        NewFact {
            fields: [("done_on".to_string(), "2026-04-18".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(subject.clone(), "the annual service", date(2026, 4, 18))
        },
    )
    .await;
    edit(
        store,
        &captured.address(),
        FactPatch {
            clear_fields: vec!["done_on".to_string()],
            ..Default::default()
        },
    )
    .await;

    let seen = read_back(store, &subject, &captured.id).await;
    assert!(
        !seen.fields.contains_key("done_on"),
        "the cleared key is not current truth any more: {seen:?}"
    );
    let history = store
        .history(&subject, "done_on")
        .await
        .expect("history should succeed");
    assert_eq!(
        history
            .iter()
            .map(|w| w.value.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("2026-04-18"), None],
        "the write that set it and the write that took it off, in that order: {history:?}"
    );
}

/// 🚨 **A correction leaves what it replaced readable, and a claim nobody
/// corrected has ONE write.**
///
/// Edit-in-place is the surface: the record reads back changed, with no
/// second copy beside it. Underneath, every write of the claim is kept, so
/// a session can tell a claim nobody ever made from one somebody made and
/// corrected.
///
/// ⚠️ **The negative is what gives it meaning.** A store that returned a
/// chain for everything would satisfy the positive and say nothing, and it
/// is what a reader would meet on every claim they ever looked at.
///
/// **The whole claim is versioned, not its words.** A correction that
/// promotes a guess to the operator's own word is a write like any other,
/// so the provenance of the earlier write is asserted here too.
pub async fn a_correction_keeps_what_the_claim_used_to_say<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-corrected");
    let corrected = capture(
        store,
        NewFact {
            provenance: Provenance::Inference,
            ..NewFact::about(subject.clone(), "was at the fair", date(2026, 4, 18))
        },
    )
    .await;
    let untouched = capture(
        store,
        NewFact::about(subject.clone(), "walked home afterwards", date(2026, 4, 18)),
    )
    .await;

    store
        .update_fact(
            &corrected.address(),
            FactPatch {
                content: Some("was never at the fair".into()),
                // The operator's own word, so the correction promotes the
                // claim — which is the second thing a write versions.
                provenance: Some(Provenance::Testimony),
                confirmed_by_user: true,
                ..Default::default()
            },
        )
        .await
        .expect("update_fact should succeed")
        .written()
        .expect("the guard waves it through");

    let chain = store
        .claim_history(&corrected.address())
        .await
        .expect("claim_history should succeed");
    assert_eq!(
        chain
            .iter()
            .map(|write| write.content.as_str())
            .collect::<Vec<_>>(),
        vec!["was at the fair", "was never at the fair"],
        "oldest first: the words the correction replaced are gone: {chain:?}"
    );
    assert_eq!(
        chain.iter().map(|write| write.ordinal).collect::<Vec<_>>(),
        vec![1, 2],
        "the order is what the substrate knows in place of a moment: {chain:?}"
    );
    assert_eq!(
        chain
            .iter()
            .map(|write| write.provenance)
            .collect::<Vec<_>>(),
        vec![Provenance::Inference, Provenance::Testimony],
        "the whole claim is versioned, so a promoted guess reads as one: {chain:?}"
    );

    // **The claim itself is what its newest write says**, which is the
    // half that keeps the surface edit-in-place.
    let read = store
        .recall(&subject)
        .await
        .expect("recall should succeed")
        .into_iter()
        .find(|f| f.id == corrected.id)
        .expect("the corrected claim is still there");
    assert_eq!(read.content, "was never at the fair");

    // ⚠️ **The negative.**
    let alone = store
        .claim_history(&untouched.address())
        .await
        .expect("claim_history should succeed");
    assert_eq!(
        alone.len(),
        1,
        "a claim nobody corrected carries a chain, so every claim a reader meets would:              {alone:?}"
    );
    assert_eq!(alone[0].content, "walked home afterwards");
}

/// 🚨 **Each write of a claim records the moment IT happened.**
///
/// Every write row used to copy the claim's own first-recorded moment, so
/// a claim corrected three times reported three writes at one instant.
/// The history is readable now, and a reader takes a field at face value:
/// it read as *these happened at once*, which is false, where saying
/// nothing would have been true. **A wrong field is worse than an absent
/// one.**
///
/// ⚠️ **The negative is load-bearing**: the claim's own first-recorded
/// moment is NOT disturbed by a later write. A caller asking when jojobot
/// took a record in is asking a real question, and this must not answer it
/// with the day somebody corrected the wording.
pub async fn each_write_of_a_claim_records_its_own_moment<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-stamped");
    let claim = capture(
        store,
        NewFact::about(subject.clone(), "was at the fair", date(2026, 4, 18)),
    )
    .await;
    let taken_in = claim
        .inserted_at
        .expect("a store stamps when it took the record in");

    store
        .update_fact(
            &claim.address(),
            FactPatch {
                content: Some("was never at the fair".into()),
                ..Default::default()
            },
        )
        .await
        .expect("update_fact should succeed")
        .written()
        .expect("the guard waves it through");

    let chain = store
        .claim_history(&claim.address())
        .await
        .expect("claim_history should succeed");
    let moments: Vec<_> = chain.iter().map(|write| write.written_at).collect();
    assert!(
        moments.iter().all(Option::is_some),
        "a write does not say when it happened: {chain:?}"
    );
    assert!(
        moments[0] < moments[1],
        "both writes report one moment, so a chain reads as corrections made at once: \
         {chain:?}"
    );

    // ⚠️ **The claim's own moment is untouched.** It says when jojobot took
    // the record in, and a correction is not that.
    let read = store
        .recall(&subject)
        .await
        .expect("recall should succeed")
        .into_iter()
        .find(|f| f.id == claim.id)
        .expect("the claim is still there");
    assert_eq!(
        read.inserted_at,
        Some(taken_in),
        "the edit re-stamped when jojobot first took the record in",
    );
}

/// **A record nobody wrote is a miss, and a handle nobody created is an
/// entity miss** — the two nothings, and the positive they rest on.
///
/// A claim that stands has at least one write, so there is no such thing
/// here as a record with an empty chain: an empty answer would be a store
/// whose substrate was never filled, and reporting it as "this record says
/// nothing" would hide that.
pub async fn claim_history_of_no_record_is_a_miss_and_of_no_entity_is_an_entity_miss<M: Memory>(
    store: &M,
) {
    let subject = EntityId::person("person:contract-chainless");
    let written = capture(
        store,
        NewFact::about(subject.clone(), "was at the yard", date(2026, 4, 18)),
    )
    .await;
    assert_eq!(
        store
            .claim_history(&written.address())
            .await
            .expect("claim_history should succeed")
            .len(),
        1,
        "the record that was written comes back — the positive the absences rest on"
    );

    let no_record = store
        .claim_history(&FactAddress::new(subject.clone(), FactId("f404".into())))
        .await;
    assert!(
        matches!(no_record, Err(MemoryError::UnknownFact { .. })),
        "a record nobody wrote is a miss, exactly as an edit of one answers: {no_record:?}"
    );
    let no_entity = store
        .claim_history(&FactAddress::new(
            EntityId::person("person:contract-no-such-chain"),
            FactId("f1".into()),
        ))
        .await;
    assert!(
        matches!(no_entity, Err(MemoryError::UnknownEntity { .. })),
        "a handle that names nothing is an entity miss, not a record one: {no_entity:?}"
    );
}

/// **A key nobody wrote is an empty history; an entity nobody created is a
/// miss.**
///
/// The two nothings a caller has to tell apart, and the positive they rest
/// on: an empty list means the thing is there and nothing was recorded
/// under that key, which is a different instruction from "that handle is
/// wrong".
pub async fn history_of_an_unwritten_key_is_empty_and_of_no_entity_is_a_miss<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-unwritten");
    capture(
        store,
        NewFact {
            fields: [("weight".to_string(), "11".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(subject.clone(), "weighed at the yard", date(2026, 4, 18))
        },
    )
    .await;

    assert_eq!(
        store
            .history(&subject, "weight")
            .await
            .expect("history should succeed")
            .len(),
        1,
        "the key that was written comes back — the positive the absences rest on"
    );
    assert!(
        store
            .history(&subject, "height")
            .await
            .expect("a key nobody wrote is an answer, not a failure")
            .is_empty(),
        "nothing was recorded under that key, and the thing is still there"
    );
    let missed = store
        .history(&EntityId::person("person:contract-no-such"), "weight")
        .await;
    assert!(
        matches!(missed, Err(MemoryError::UnknownEntity { .. })),
        "a handle that names nothing is a miss, exactly as recall answers one: {missed:?}"
    );
}

/// `update_fact` attaches an edge to a fact that didn't have one — the
/// day-to-day path for an edge realized after the fact was captured.
pub async fn update_fact_attaches_an_edge<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-edge-later");
    let captured = capture(
        store,
        NewFact::about(subject.clone(), "was at the festival", date(2026, 7, 1)),
    )
    .await;
    assert_eq!(captured.edge, None);

    let edge = Edge::new(
        EdgeShape::Attendance,
        EntityId("event:contract-winter-fest".into()),
    );
    let updated = edit(
        store,
        &captured.address(),
        FactPatch {
            edge: Some(edge.clone()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(updated.edge.as_ref(), Some(&edge));
    assert_eq!(
        read_back(store, &subject, &captured.id).await.edge.as_ref(),
        Some(&edge),
        "the attached edge must be on the read path"
    );
}

/// **A record's fields survive capture through any store, and no label is
/// asked for.**
///
/// Every other spec for these fields in this workspace runs against
/// something that holds the record in memory, so all of them can pass while
/// the one store production actually writes to drops the bag on the floor.
/// That is not hypothetical: it is what the Outline adapter did — it built
/// its stored fact field by field and left the bag out, so a capture
/// answered with a record it had not written and a restart read back a
/// record with no fields.
///
/// **The read-back guard cannot catch this one**, which is why it needs a
/// spec of its own. Read-back compares what came back against what the
/// adapter *believed* it stored, so a write that drops the fields on both
/// halves passes its own invariant. The
/// comparison a dropped field cannot survive is against the CALLER's
/// record, and this is the only place that comparison is made.
pub async fn a_records_fields_survive_capture<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-evented");
    let touched = EntityId("place:contract-kiln-yard".into());
    ensure(store, &touched).await;

    let recorded: std::collections::BTreeMap<String, String> = [
        ("mood".to_string(), "delighted".to_string()),
        // A key no build has ever heard of, because the promise is that an
        // unknown field is kept as written rather than that known fields
        // survive.
        (
            "a-field-from-a-later-build".to_string(),
            "and its value".to_string(),
        ),
        // **The punctuation battery, and it is not decoration.** Every
        // character below was mangled by a real store at some point in this
        // record's short life: a space and an `=`, which a store rewrote
        // when it re-serialized the escapes around them, and a `~`, in
        // front of which a store INSERTED an escape of its own. A fake that
        // stores bytes verbatim finds none of this, which is why the battery
        // rides in the shared contract rather than in an adapter's own
        // tests.
        (
            "punctuation".to_string(),
            "a = b, c~d, <e> & \"f\" — 100% ünïcode".to_string(),
        ),
        // **The longest key a caller may write, and that is not
        // decoration.** A key crosses to a store as a column value, and
        // the column carrying it is part of a primary key — so keys do not
        // all cost the same, and a store that holds a short one can refuse
        // a long one on the write. A case that picks a short key answers
        // for short keys only. The fake keeps a map in memory and has no
        // cell to overflow, so it answers the same either way: only a
        // store can answer for this, which is why the key rides in the
        // contract both stores run.
        ("k".repeat(MAX_KEY_CHARS), "at the limit".to_string()),
    ]
    .into_iter()
    .collect();
    let captured = capture(
        store,
        NewFact {
            fields: recorded.clone(),
            refs: vec![touched.clone()],
            ..NewFact::about(
                subject.clone(),
                "the kiln was finally lit",
                date(2026, 7, 2),
            )
        },
    )
    .await;
    assert_eq!(
        captured.fields, recorded,
        "capture answered with fields it did not store"
    );

    let seen = read_back(store, &subject, &captured.id).await;
    assert_eq!(
        seen, captured,
        "the fields must survive read-back byte-identical"
    );
    assert_eq!(
        seen.fields, recorded,
        "…and they are what a later reader takes off the record"
    );
    assert_eq!(
        seen.refs,
        vec![touched],
        "the unnamed references survive with them"
    );
}

/// **An event's refs name entities, so the guard screens them too.**
///
/// The rule is not about edges, it is about naming: nothing a write
/// mentions is brought into being — or waved through unrecognized — as a
/// side effect of mentioning it. A store that screened the subject and the
/// edge object but not the refs would make the open hatch the one door on
/// this surface where naming a stranger is free, and the hatch is ungated
/// on its TYPE precisely so that everything else about it stays strict.
///
/// And it takes the whole write with it: a record is one write, so a ref
/// that cannot be resolved leaves no half-recorded fact behind.
pub async fn a_records_ref_is_screened_by_the_guard<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-ref-guarded");
    ensure(store, &subject).await;
    let stranger = EntityId::person("person:contract-nobody-created-this");

    let outcome = store
        .capture(NewFact {
            refs: vec![stranger.clone()],
            ..NewFact::about(subject.clone(), "should not land", date(2026, 7, 2))
        })
        .await
        .expect("the call itself succeeds; the guard answers in the result");
    let Guarded::Blocked { attempted, .. } = outcome else {
        panic!("a ref naming an entity nobody created must be blocked");
    };
    assert_eq!(attempted, stranger, "the guard names the handle it stopped");
    assert_nothing_recorded(store, &subject).await;
}

/// **The key jojobot writes itself is refused to a caller, not silently
/// eaten.**
///
/// `retracts` names the row a retraction takes back, and jojobot is the
/// only writer of it: a caller able to write that key could mark somebody
/// else's record taken back without going through the verb that decides
/// whether it may be.
///
/// **The spec belongs in the shared contract because the refusal is the
/// domain's**, so both stores answer for it: a rule one store enforces and
/// the other does not is a rule that holds until somebody switches stores.
pub async fn a_reserved_field_key_is_refused<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-reserved-key");
    ensure(store, &subject).await;

    let outcome = store
        .capture(NewFact {
            fields: [("retracts".to_string(), "person:someone-else#f1".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(subject.clone(), "it happened", date(2026, 7, 3))
        })
        .await;
    assert!(
        matches!(outcome, Err(MemoryError::InvalidFact(_))),
        "a field named `retracts` must be refused rather than forging the marker \
         that says a record was taken back: {outcome:?}"
    );

    // **The refusal is served to the caller verbatim, so it teaches
    // whatever it says.** A record is a claim; whether this store keeps one
    // as a line in a table is the store's business and a word that stops
    // being true the day the product underneath is swapped. An agent taught
    // it has to unlearn a model rather than read a new sentence.
    let said = outcome.expect_err("refused above").to_string();
    assert!(
        said.contains("retracts"),
        "the refusal names the key it stopped, or the caller cannot act on it: {said}"
    );
    let word = |needle: &str| {
        said.split(|c: char| !c.is_alphanumeric())
            .any(|w| w.eq_ignore_ascii_case(needle))
    };
    assert!(
        !(word("row") || word("rows")),
        "the refusal calls the thing taken back a row, which is this store's furniture \
         rather than what a caller holds: {said}"
    );
    // **The noun on the thing taken back, not merely somewhere in the
    // sentence.** The refusal opens by saying what a record's fields are,
    // so a needle anywhere in the text is satisfied by that opening and
    // stays green while the clause at issue says anything at all — the fix
    // here is never deletion.
    assert!(
        said.contains("names the record"),
        "…and what the key names is a record, said where it is named: dropping the noun \
         costs the caller the only sentence that says what the key holds: {said}"
    );

    // …and the ordinary keys beside it are untouched, `type` and `ref`
    // included: those words belong to the edge grammar, not to a record's
    // own fields.
    let landed = capture(
        store,
        NewFact {
            fields: [
                ("kind".to_string(), "a value".to_string()),
                ("type".to_string(), "another".to_string()),
                ("ref".to_string(), "a third".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(subject.clone(), "it happened later", date(2026, 7, 4))
        },
    )
    .await;
    assert_eq!(
        landed.fields.get("kind").map(String::as_str),
        Some("a value")
    );
    assert_eq!(
        landed.fields.get("type").map(String::as_str),
        Some("another")
    );
    assert_eq!(
        landed.fields.get("ref").map(String::as_str),
        Some("a third")
    );
}

/// **Retraction: the record stays, marked, and the reason lands beside
/// it.** Nothing is removed — that is the no-delete rule, and it is what
/// makes this different from every store where taking something back means
/// losing the evidence that it was ever said.
pub async fn retracting_a_record_marks_it_and_records_why<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-retracted");
    let event = capture(
        store,
        NewFact::about(subject.clone(), "moved to the 14th", date(2026, 7, 3)),
    )
    .await;

    let taken_back = store
        .retract(
            &event.address(),
            Some("it was rebooked twice"),
            date(2026, 7, 4),
        )
        .await
        .expect("retracting a record should succeed");

    // The row it names: same id, same words, same place.
    assert_eq!(taken_back.retracted.id, event.id);
    assert_eq!(taken_back.retracted.content, event.content);
    assert_eq!(taken_back.retracted.fields, event.fields);
    assert_eq!(
        taken_back.retracted.status,
        FactStatus::Archived,
        "the row is marked rather than removed"
    );

    // And the account of why, as a record of its own.
    assert_eq!(taken_back.record.content, "it was rebooked twice");
    assert_eq!(taken_back.record.recorded_at, date(2026, 7, 4));
    assert_eq!(
        taken_back.record.retracts(),
        Some(event.address().to_string().as_str()),
        "the retraction names what it takes back, or the two are not one story"
    );

    // Both are on the read path, which is what makes any of it durable.
    let seen = read_back(store, &subject, &event.id).await;
    assert_eq!(seen.status, FactStatus::Archived);
    assert_eq!(
        seen.content, event.content,
        "the words are untouched: it is marked, not edited"
    );
    let account = read_back(store, &subject, &taken_back.record.id).await;
    assert_eq!(account, taken_back.record);
}

/// **A retraction with no reason still records the act, and says the
/// reason is missing rather than inventing one.**
///
/// The reason is optional and a row still has to carry content, so the
/// absent case writes a sentence either way. The risk it leaves behind is that the sentence is jojobot's
/// and not a caller's: it must state only what happened and that nobody
/// said why, because a plausible-sounding reason here would be
/// indistinguishable later from one somebody actually gave.
pub async fn a_retraction_needs_no_reason<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-unreasoned");
    let event = capture(
        store,
        NewFact::about(subject.clone(), "it happened", date(2026, 7, 3)),
    )
    .await;

    let taken_back = store
        .retract(&event.address(), None, date(2026, 7, 4))
        .await
        .expect("a retraction without a reason is still a retraction");

    // The act landed in full: the row is marked and the account is a real
    // record, linked, dated, and on the read path like any other.
    assert_eq!(taken_back.retracted.status, FactStatus::Archived);
    assert_eq!(
        taken_back.record.retracts(),
        Some(event.address().to_string().as_str()),
    );
    assert_eq!(
        read_back(store, &subject, &taken_back.record.id).await,
        taken_back.record,
    );

    // And the content says the reason is absent — it does not stand in for
    // one. Asserted as the two things a later reader needs to be able to
    // tell apart, rather than as an exact string nobody may reword.
    let content = taken_back.record.content.to_lowercase();
    assert!(
        content.contains("retracted"),
        "the record has to say what happened: {content:?}"
    );
    assert!(
        content.contains("no reason"),
        "…and that nobody gave a reason, rather than supplying one: {content:?}"
    );
}

/// **One-way, and the three ways of asking for the reverse are all
/// refused.** Retracting twice, retracting the retraction, and editing the
/// status back are the same wish wearing three faces — so no single one of
/// them is the whole test.
pub async fn a_retraction_is_one_way<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-oneway");
    let event = capture(
        store,
        NewFact::about(subject.clone(), "it happened", date(2026, 7, 3)),
    )
    .await;
    let taken_back = store
        .retract(
            &event.address(),
            Some("it did not, in fact"),
            date(2026, 7, 4),
        )
        .await
        .expect("the first retraction lands");

    let again = store
        .retract(&event.address(), Some("again"), date(2026, 7, 5))
        .await;
    // **Refused as already-done, never as impossible.** The caller asked
    // for a state jojobot is holding: an answer that says the retraction
    // did not happen and cannot happen is false about the record and about
    // what to do next.
    assert!(
        matches!(again, Err(MemoryError::AlreadyRetracted { .. })),
        "a second retraction is refused because it is already so: {again:?}"
    );

    let the_record = store
        .retract(&taken_back.record.address(), Some("undo"), date(2026, 7, 5))
        .await;
    assert!(
        matches!(the_record, Err(MemoryError::NotRetractable { .. })),
        "a retraction is not itself retractable: {the_record:?}"
    );

    // The edit path is the third face, and the one a caller reaches for
    // without meaning anything by it.
    let edited = store
        .update_fact(
            &event.address(),
            FactPatch {
                status: Some(FactStatus::Active),
                ..Default::default()
            },
        )
        .await;
    assert!(
        matches!(edited, Err(MemoryError::NotRetractable { .. })),
        "a retracted row is not editable back to active: {edited:?}"
    );
    assert_eq!(
        read_back(store, &subject, &event.id).await.status,
        FactStatus::Archived,
        "and none of the three moved it"
    );
}

/// **The marker is machinery, and machinery is not one of the thing's
/// properties.**
///
/// A retraction account is an active record, so its writes fold like any
/// other's — and the key it carries is the one no caller may write. Folded
/// into the dense row it reads as the thing's own property: an agent is
/// told that what this thing IS is a fact address, the operator's page
/// renders it under what the thing is, and the index takes a posting for
/// it.
///
/// The record keeps it. That is where the marker means something, and
/// [`Fact::is_retraction`] is read off it.
pub async fn the_retraction_marker_is_not_one_of_the_things_fields<M: Memory>(store: &M) {
    let subject = EntityId("thing:contract-marker-not-a-field".into());
    let claim = capture(
        store,
        NewFact {
            fields: [("cost".to_string(), "40".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(subject.clone(), "bought at the counter", date(2026, 4, 18))
        },
    )
    .await;
    assert_eq!(
        thing_fields(store, &subject).await.get("cost"),
        Some(&"40".to_string()),
        "the setup has to have landed, or what is asserted below passes on nothing"
    );

    let taken_back = store
        .retract(
            &claim.address(),
            Some("the receipt said otherwise"),
            date(2026, 4, 19),
        )
        .await
        .expect("retracting the claim should succeed");

    let held = thing_fields(store, &subject).await;
    assert!(
        !held.contains_key(RETRACTS),
        "the key jojobot writes to mark a record taken back is not a property of the \
         thing: {held:?}"
    );
    assert!(
        !held.contains_key("cost"),
        "…and the retracted claim's own key went with it: {held:?}"
    );

    // The other half: it did not leave the record, which is the only place
    // it ever meant anything.
    let account = read_back(store, &subject, &taken_back.record.id).await;
    assert_eq!(
        account.retracts(),
        Some(claim.address().to_string().as_str()),
        "the account still names what it took back"
    );
    assert!(
        account.is_retraction(),
        "…and still reads as a retraction: {account:?}"
    );
}

/// **Taking the marker off is refused the way writing it is.**
///
/// The set path screens the reserved key so the marker cannot be forged.
/// The clear path is the same key on the same rail: an edit that removes it
/// makes [`Fact::is_retraction`] answer false, which lets the account be
/// retracted — the reversal the one-way rule exists to forbid — and takes
/// away the only machine-readable link from the marked row to its account.
///
/// **The refusal is one move, not a lockout.** The account is an ordinary
/// record otherwise, and the repairs any record gets still reach it.
pub async fn clearing_the_retraction_marker_is_refused<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-clear-marker");
    let event = capture(
        store,
        NewFact::about(subject.clone(), "it happened", date(2026, 7, 3)),
    )
    .await;
    let taken_back = store
        .retract(
            &event.address(),
            Some("it did not, in fact"),
            date(2026, 7, 4),
        )
        .await
        .expect("the retraction lands");
    let account = taken_back.record.address();

    let refused = store
        .update_fact(
            &account,
            FactPatch {
                clear_fields: vec![RETRACTS.to_string()],
                ..Default::default()
            },
        )
        .await;
    assert!(
        matches!(refused, Err(MemoryError::InvalidFact(_))),
        "clearing the reserved key must be refused as hard as writing it: {refused:?}"
    );

    // **The refusal is served verbatim, so it teaches whatever it says** —
    // the same pin the set path carries, because both are text an agent
    // reads and acts on. Read once and asserted on both halves at once: the
    // key it stopped, which is what says this refusal and not some other
    // error came back, and the way out, without which a caller holding a
    // retraction it regrets has nowhere to go but a second attempt at the
    // move that was just refused.
    let said = refused.expect_err("refused above").to_string();
    assert!(
        said.contains(RETRACTS),
        "the refusal names the key it stopped, or the caller cannot tell which \
         refusal this is: {said}"
    );
    assert!(
        said.contains("capture what is so now as a new record"),
        "…and it hands back the move that IS allowed — a new record saying what is \
         so — because the one it refused is the only one a caller would try next: {said}"
    );

    // And nothing moved: the account is still a retraction, so the row it
    // took back is still linked to it and it is still not retractable.
    let still = read_back(store, &subject, &taken_back.record.id).await;
    assert_eq!(
        still.retracts(),
        Some(event.address().to_string().as_str()),
        "the refused edit wrote nothing"
    );
    let reversal = store
        .retract(&account, Some("undo"), date(2026, 7, 5))
        .await;
    assert!(
        matches!(reversal, Err(MemoryError::NotRetractable { .. })),
        "a retraction of a retraction stays refused: {reversal:?}"
    );

    // The floor rule's other half: the screen refuses the one move, not the
    // record. An ordinary key on the account is set and cleared as usual.
    edit(
        store,
        &account,
        FactPatch {
            fields: [("filed_by".to_string(), "the desk".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        },
    )
    .await;
    let repaired = edit(
        store,
        &account,
        FactPatch {
            clear_fields: vec!["filed_by".to_string()],
            ..Default::default()
        },
    )
    .await;
    assert!(
        !repaired.fields.contains_key("filed_by"),
        "an ordinary key on a retraction account is still the caller's to clear: {repaired:?}"
    );
    assert!(
        repaired.is_retraction(),
        "…and the repair left the marker where it was: {repaired:?}"
    );
}

/// An address naming nothing is the same miss an edit's is — never a new
/// record, and never a silent success.
pub async fn retracting_an_unknown_address_never_writes<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-retract-miss");
    ensure(store, &subject).await;
    let missed = FactAddress::new(subject.clone(), FactId("f404".into()));

    let refused = store
        .retract(&missed, Some("nothing here"), date(2026, 7, 4))
        .await;
    assert!(
        matches!(refused, Err(MemoryError::UnknownFact { .. })),
        "a missed address is a miss, not a create: {refused:?}"
    );
    assert!(
        !store
            .recall(&subject)
            .await
            .expect("recall should succeed")
            .iter()
            .any(|f| f.id == missed.local),
        "nothing was written at the address that missed"
    );
}

// --- the write guard, on the write path ----------------------------------

/// 🚨 **An exact handle collision is refused and stays refused — no token
/// clears it — and a near-miss is caught and its own override lifts it —
/// asked of a stored row and a record the build supplies, in one call**
/// (rule 234).
///
/// **Both stores are arguments, not a choice of two functions.** A caller
/// cannot supply the stored half alone: the supplied half is not a second
/// entry point somebody has to remember to also call, so it cannot be
/// silently dropped from a suite that calls this one — a gap no assertion
/// can catch on its own, because a call nobody made leaves nothing to
/// fail. Two same-named people can never merge into one portrait silently
/// (rule 61) is the stored answer; the collision refusing a shipped name
/// even harder is the supplied one. Written once against a
/// [`support::Backing`], never twice by hand — two hand-written copies of
/// this already existed and had already drifted: the stored one checked a
/// candidate's reason and its source, the supplied one checked neither.
pub async fn add_entity_guards_hold_for_stored_and_supplied<M: Memory, S: Memory>(
    stored: &M,
    supplied: &S,
) {
    add_entity_guards_hold_for(
        stored,
        &support::Stored {
            handle: EntityId::person("person:contract-alpha"),
            name: "Alpha",
            source: "crm-card",
        },
    )
    .await;
    add_entity_guards_hold_for(supplied, &support::Supplied).await;
}

async fn add_entity_guards_hold_for<M: Memory, B: support::Backing<M>>(store: &M, backing: &B) {
    let (existing, source) = backing.existing(store).await;
    let kind = existing.kind().expect("an existing handle names a kind");

    // ── exact collision: refused, and no token clears it ──────────────
    let outcome = store
        .add_entity(NewEntity::new(
            existing.clone(),
            "Somebody Else's Name",
            "user-named",
        ))
        .await
        .expect("the call itself succeeds; the guard answers in the result");
    let Guarded::Blocked {
        attempted,
        candidates,
    } = outcome
    else {
        panic!("an exact handle collision must be refused: {existing}");
    };
    assert_eq!(
        candidates[0].reason,
        guard::MatchReason::ExactHandle,
        "the candidate's reason must name an exact match: {candidates:?}",
    );
    assert_eq!(
        candidates[0].source, source,
        "the candidate must report the existing thing's own source: {candidates:?}",
    );
    let token = guard::override_token(&attempted, &candidates);
    let forced = store
        .add_entity(NewEntity {
            override_token: Some(token),
            ..NewEntity::new(existing.clone(), "Somebody Else's Name", "user-named")
        })
        .await
        .expect("the call itself succeeds; the guard answers in the result");
    assert!(
        matches!(forced, Guarded::Blocked { .. }),
        "an exact handle collision is never overridable: {forced:?}",
    );
    // **The read every existing thing answers, stored or supplied** — a
    // whole-entity lookup like [`read_entity`]'s is `list_entities`-backed,
    // and `list_entities` is deliberately narrower than the existence gate
    // (`Provisioned`'s to widen, not this contract's to test), so it cannot
    // stand in for both backings here.
    assert!(
        store.fields(&existing).await.is_ok(),
        "the existing thing must still resolve after a refused write: {existing}",
    );

    // ── near miss: caught, and its own override lifts it ──────────────
    let slug = existing.slug();
    let near = EntityId::new(kind, &slug[..slug.len() - 1]);
    let outcome = store
        .add_entity(NewEntity::new(near.clone(), "Near Miss", "user-named"))
        .await
        .expect("the call itself succeeds; the guard answers in the result");
    let Guarded::Blocked {
        attempted,
        candidates,
    } = outcome
    else {
        panic!("a one-edit-off handle must be reported: {near}");
    };
    assert!(
        candidates.iter().any(|m| m.handle == existing),
        "the existing thing itself must be the candidate: {candidates:?}",
    );
    assert!(
        store
            .list_entities(Some(kind))
            .await
            .expect("list_entities should succeed")
            .iter()
            .all(|e| e.id != near),
        "a blocked add must write nothing",
    );
    assert!(
        matches!(
            store
                .add_entity(NewEntity {
                    override_token: Some("0000000000000000".into()),
                    ..NewEntity::new(near.clone(), "Near Miss", "user-named")
                })
                .await
                .expect("the call itself succeeds; the guard answers in the result"),
            Guarded::Blocked { .. }
        ),
        "a token nobody minted lifts nothing",
    );
    let token = guard::override_token(&attempted, &candidates);
    let lifted = add(
        store,
        NewEntity {
            override_token: Some(token),
            ..NewEntity::new(near.clone(), "Near Miss", "user-named")
        },
    )
    .await;
    assert_eq!(
        lifted.id, near,
        "the refusal's own token must let a genuinely different thing through",
    );
}

/// 🚨 **A LISTING answers for a stored thing and a supplied one alike —
/// asked of both in one call** (rule 234): naming a kind, or naming none,
/// has to return everything the store holds of it AND everything the
/// build supplies — a listing that answered only for rows would make a
/// shipped view invisible to the one verb whose whole job is saying what
/// is there.
///
/// **Both stores are arguments, not a choice of two functions** — see
/// [`add_entity_guards_hold_for_stored_and_supplied`], the same shape for
/// the same reason: the supplied half cannot be silently dropped from a
/// suite that calls this one. `list_entities` resolving what the build
/// supplies is a supplied-resolving decorator's job, not the bare store's,
/// so the caller passes a bare store for the stored half and one that
/// resolves supplied records for the other — each asked the question it
/// can actually answer.
pub async fn an_existing_thing_appears_in_its_kinds_listing_stored_and_supplied<
    M: Memory,
    S: Memory,
>(
    stored: &M,
    supplied: &S,
) {
    an_existing_thing_is_in_its_kinds_listing(
        stored,
        &support::Stored {
            handle: EntityId("thing:contract-listed-thing".into()),
            name: "Listed",
            source: "user-named",
        },
    )
    .await;
    an_existing_thing_is_in_its_kinds_listing(supplied, &support::Supplied).await;
}
async fn an_existing_thing_is_in_its_kinds_listing<M: Memory, B: support::Backing<M>>(
    store: &M,
    backing: &B,
) {
    let (existing, _source) = backing.existing(store).await;
    let kind = existing.kind().expect("an existing handle names a kind");
    assert!(
        store
            .list_entities(Some(kind))
            .await
            .expect("list_entities should succeed")
            .iter()
            .any(|e| e.id == existing),
        "the existing thing must appear in its kind's listing: {existing}",
    );
    assert!(
        store
            .list_entities(None)
            .await
            .expect("list_entities should succeed")
            .iter()
            .any(|e| e.id == existing),
        "the existing thing must appear in the unfiltered listing: {existing}",
    );
    // **The negative half of the same read**: a kind nothing here answers
    // to selects nothing, stored or supplied — a listing that returned
    // everything regardless of the filter would pass the assertions above
    // for the wrong reason.
    let other = if kind == EntityKind::THING {
        EntityKind::PERSON
    } else {
        EntityKind::THING
    };
    assert!(
        store
            .list_entities(Some(other))
            .await
            .expect("list_entities should succeed")
            .iter()
            .all(|e| e.id != existing),
        "the existing thing appeared under a kind it does not answer to: {existing}",
    );
}

/// Capture's subject must already exist. Not "must not look like
/// something else" — must *be* something: letting a novel subject
/// self-provision a nameless entity would turn every typo or
/// plausible-looking AI handle into a permanent record nobody chose.
/// There is no override on this path either: a genuinely new
/// entity is `add_entity`, then the capture — two deliberate steps.
pub async fn capture_requires_an_existing_subject<M: Memory>(store: &M) {
    let known = EntityId::person("person:contract-zenith");
    add(store, NewEntity::new(known.clone(), "Zenith", "user-named")).await;

    // A fact about an entity that exists: waved straight through, always —
    // otherwise every second fact about someone would need confirming.
    capture(
        store,
        NewFact::about(known.clone(), "likes long walks", date(2026, 7, 1)),
    )
    .await;

    // A near miss comes back with the candidate that explains it…
    let typo = EntityId::person("person:contract-zenit");
    let outcome = store
        .capture(NewFact::about(
            typo.clone(),
            "should not land",
            date(2026, 7, 1),
        ))
        .await
        .expect("the call itself succeeds; the guard answers in the result");
    let Guarded::Blocked { candidates, .. } = outcome else {
        panic!("a near-miss subject must be reported, never provisioned");
    };
    assert!(
        candidates.iter().any(|m| m.handle == known),
        "got {candidates:?}"
    );

    // …and a handle nothing resembles blocks just the same, with nothing
    // to suggest.
    let stranger = EntityId("work:contract-first-mix".into());
    let outcome = store
        .capture(NewFact::about(
            stranger.clone(),
            "32 tracks",
            date(2026, 7, 1),
        ))
        .await
        .expect("the call itself succeeds; the guard answers in the result");
    let Guarded::Blocked {
        attempted,
        candidates,
    } = outcome
    else {
        panic!("an unknown subject must block even with no near match");
    };
    assert_eq!(attempted, stranger, "the guard names the handle it stopped");
    assert!(
        candidates.is_empty(),
        "nothing resembles it: {candidates:?}"
    );

    for blocked in [&typo, &stranger] {
        assert_nothing_recorded(store, blocked).await;
        // …and no entity either. Checking only for facts left the guard's
        // "write NOTHING" half-tested: an adapter that provisioned the doc
        // before screening would still show an empty fact table here.
        assert!(
            store
                .list_entities(None)
                .await
                .expect("list")
                .iter()
                .all(|e| &e.id != blocked),
            "a blocked capture must not have provisioned the entity either ({blocked})"
        );
    }

    // The way through is to mean it: add the entity, then capture.
    add(
        store,
        NewEntity::new(stranger.clone(), "First Mix", "user-named"),
    )
    .await;
    let landed = capture(
        store,
        NewFact::about(stranger.clone(), "32 tracks", date(2026, 7, 1)),
    )
    .await;
    assert_eq!(landed.subject, stranger);
    assert_eq!(
        read_back(store, &stranger, &landed.id).await.content,
        "32 tracks"
    );
    assert_eq!(
        read_entity(store, &stranger).await.source,
        "user-named",
        "existence is sourced by whoever asked for it, never by a side effect"
    );
}

/// The same gate on an **edge's object**: a handle nothing resembles is
/// refused rather than quietly becoming a new node. This is where ask-across
/// rots silently — the edge points at something nobody else references, the
/// walk comes back empty, and nothing looks wrong.
pub async fn capture_requires_an_existing_edge_object<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-edge-stranger");
    add(
        store,
        NewEntity::new(subject.clone(), "Edge Stranger", "user-named"),
    )
    .await;

    let stranger = EntityId("event:contract-unheard-of-fest".into());
    let outcome = store
        .capture(NewFact {
            edge: Some(Edge::new(EdgeShape::Attendance, stranger.clone())),
            ..NewFact::about(subject.clone(), "should not land", date(2026, 7, 1))
        })
        .await
        .expect("the call itself succeeds; the guard answers in the result");
    let Guarded::Blocked {
        attempted,
        candidates,
    } = outcome
    else {
        panic!("an unknown edge object must block even with no near match");
    };
    assert_eq!(attempted, stranger);
    assert!(
        candidates.is_empty(),
        "nothing resembles it: {candidates:?}"
    );
    assert_nothing_recorded(store, &subject).await;
    assert!(
        store
            .list_entities(None)
            .await
            .expect("list")
            .iter()
            .all(|e| e.id != stranger),
        "…and must not have provisioned the object either"
    );

    add(
        store,
        NewEntity::new(stranger.clone(), "Unheard-of Fest", "user-named"),
    )
    .await;
    let landed = capture(
        store,
        NewFact {
            edge: Some(Edge::new(EdgeShape::Attendance, stranger.clone())),
            ..NewFact::about(subject.clone(), "went both nights", date(2026, 7, 1))
        },
    )
    .await;
    assert_eq!(landed.edge.map(|e| e.object), Some(stranger));
}

/// The same gate on an **edge attached later**. A path carrying the code
/// and no spec is one where deleting the check leaves the whole suite
/// green, and the hole is exactly the interesting one: an edge realized
/// after the fact is the day-to-day way edges get drawn.
pub async fn update_fact_requires_an_existing_edge_object<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-late-edge");
    let captured = capture(
        store,
        NewFact::about(subject.clone(), "was somewhere that week", date(2026, 7, 1)),
    )
    .await;

    let stranger = EntityId("place:contract-nowhere-in-particular".into());
    let outcome = store
        .update_fact(
            &captured.address(),
            FactPatch {
                edge: Some(Edge::new(EdgeShape::Location, stranger.clone())),
                ..Default::default()
            },
        )
        .await
        .expect("the call itself succeeds; the guard answers in the result");
    let Guarded::Blocked {
        attempted,
        candidates,
    } = outcome
    else {
        panic!("an edge object that names no entity must block the edit");
    };
    assert_eq!(attempted, stranger, "the guard names the handle it stopped");
    assert!(
        candidates.is_empty(),
        "nothing resembles it: {candidates:?}"
    );

    assert_eq!(
        read_back(store, &subject, &captured.id).await.edge,
        None,
        "a blocked edit must leave the fact exactly as it was"
    );
    assert!(
        store
            .list_entities(None)
            .await
            .expect("list")
            .iter()
            .all(|e| e.id != stranger),
        "…and must not have provisioned the object either"
    );

    // Two deliberate steps, here as everywhere: add the entity, then write.
    add(
        store,
        NewEntity::new(stranger.clone(), "Nowhere In Particular", "user-named"),
    )
    .await;
    let landed = edit(
        store,
        &captured.address(),
        FactPatch {
            edge: Some(Edge::new(EdgeShape::Location, stranger.clone())),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(landed.edge.map(|e| e.object), Some(stranger));
}

/// 🚨 **The handle-hijack case, decision log 268.** An edge is written at a
/// handle; that handle is renamed away; a NEW entity is created at the
/// vacated handle. Before this slice, an edge's object was stored as the
/// plain handle it was drawn with, so it silently re-resolved to the
/// newcomer the moment one existed — a pointer written at one thing
/// serving a claim about a different one, with no error and no warning.
/// Storing the badge instead removes the handle from the row entirely:
/// there is nothing left in it for a newcomer to inherit.
pub async fn a_rename_and_a_recreated_handle_does_not_hijack_an_edge<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-hijack-subject");
    let original = EntityId("thing:contract-hijack-target".into());
    ensure(store, &subject).await;
    add(
        store,
        NewEntity::new(original.clone(), "The Original", "the roster"),
    )
    .await;

    let fact = capture(
        store,
        NewFact {
            edge: Some(Edge::new(EdgeShape::About, original.clone())),
            ..NewFact::about(
                subject.clone(),
                "drew an edge at the original",
                date(2026, 4, 20),
            )
        },
    )
    .await;

    let renamed_to = EntityId("thing:contract-hijack-elsewhere".into());
    store
        .rename_entity(&original, &renamed_to, None, date(2026, 4, 21), None)
        .await
        .expect("rename should succeed")
        .written()
        .expect("nothing collides with it");

    // A DIFFERENT entity now claims the vacated handle.
    add(
        store,
        NewEntity::new(original.clone(), "The Newcomer", "the roster"),
    )
    .await;

    let after = store
        .recall(&subject)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == fact.id)
        .expect("the claim is there");
    assert_eq!(
        after.edge.as_ref().map(|e| &e.object),
        Some(&renamed_to),
        "the edge followed the rename to {renamed_to}, or the newcomer hijacked it: {after:?}",
    );
}

/// 🚨 **The same hijack case, on a record's refs.** All three of the
/// permanent-id columns hold an id rather than a name; proving it for
/// `edge.object` and leaving `refs` to rest on "the same mechanism"
/// would leave one of the three proven by argument while the other two
/// are proven by a run.
pub async fn a_rename_and_a_recreated_handle_does_not_hijack_a_ref<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-ref-vacancy-onlooker");
    let original = EntityId("thing:contract-ref-vacancy".into());
    ensure(store, &subject).await;
    add(
        store,
        NewEntity::new(original.clone(), "Krusty", "the roster"),
    )
    .await;

    let fact = capture(
        store,
        NewFact {
            refs: vec![original.clone()],
            ..NewFact::about(
                subject.clone(),
                "pointed a ref at the original",
                date(2026, 4, 20),
            )
        },
    )
    .await;

    let renamed_to = EntityId("thing:contract-ref-departed".into());
    store
        .rename_entity(&original, &renamed_to, None, date(2026, 4, 21), None)
        .await
        .expect("rename should succeed")
        .written()
        .expect("nothing collides with it");

    // A DIFFERENT entity now claims the vacated handle.
    add(
        store,
        NewEntity::new(original.clone(), "Skinner", "the roster"),
    )
    .await;

    let after = store
        .recall(&subject)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|f| f.id == fact.id)
        .expect("the claim is there");
    assert_eq!(
        after.refs,
        vec![renamed_to.clone()],
        "the ref followed the rename to {renamed_to}, or the newcomer hijacked it: {after:?}",
    );
}

/// 🚨 **The same hijack case, on `entity.parent`.** A child is parented on
/// a handle; that handle is renamed away; a NEW entity is created at the
/// vacated handle. `entity.parent` is stored as the badge the parent
/// wears (rule 268), so the child keeps naming the renamed thing rather
/// than silently adopting the newcomer.
pub async fn a_rename_and_a_recreated_handle_does_not_hijack_a_parent<M: Memory>(store: &M) {
    let original = EntityId("thing:contract-parented-vacancy".into());
    add(
        store,
        NewEntity::new(original.clone(), "Quimby", "the roster"),
    )
    .await;
    let child = EntityId("thing:contract-parented-dependent".into());
    add(
        store,
        NewEntity {
            parent: Some(original.clone()),
            ..NewEntity::new(child.clone(), "Nelson", "the roster")
        },
    )
    .await;

    let renamed_to = EntityId("thing:contract-parented-departed".into());
    store
        .rename_entity(&original, &renamed_to, None, date(2026, 4, 21), None)
        .await
        .expect("rename should succeed")
        .written()
        .expect("nothing collides with it");

    // A DIFFERENT entity now claims the vacated handle.
    add(
        store,
        NewEntity::new(original.clone(), "Wiggum", "the roster"),
    )
    .await;

    let held = store
        .list_entities(None)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|e| e.id == child)
        .expect("the child is there");
    assert_eq!(
        held.parent.as_ref(),
        Some(&renamed_to),
        "the parent followed the rename to {renamed_to}, or the newcomer hijacked it: {held:?}",
    );
}

/// **An entity keeps the other names it answers to**, through the store and
/// back. A nickname that survives only in the caller's request is a nickname
/// the next session has never heard of.
pub async fn add_entity_keeps_its_alternate_names<M: Memory>(store: &M) {
    let id = EntityId::person("person:contract-many-named");
    let added = add(
        store,
        NewEntity {
            aliases: vec!["Contract Nickname".into(), "C.M.N.".into()],
            ..NewEntity::new(id.clone(), "Contract Many-Named", "user-named")
        },
    )
    .await;
    assert_eq!(added.aliases, vec!["Contract Nickname", "C.M.N."]);
    assert_eq!(
        read_entity(store, &id).await,
        added,
        "…on the read path too"
    );

    // The set is replaced whole, and an omitted field is left alone.
    let renamed = store
        .update_entity(
            &id,
            EntityPatch {
                source: Some("crm-card".into()),
                ..Default::default()
            },
        )
        .await
        .expect("update ok")
        .written()
        .expect("not blocked");
    assert_eq!(
        renamed.aliases, added.aliases,
        "an omitted alias set is untouched"
    );

    let replaced = store
        .update_entity(
            &id,
            EntityPatch {
                aliases: Some(vec!["Only This One".into()]),
                ..Default::default()
            },
        )
        .await
        .expect("update ok")
        .written()
        .expect("not blocked");
    assert_eq!(replaced.aliases, vec!["Only This One"]);
    assert_eq!(
        read_entity(store, &id).await.aliases,
        vec!["Only This One"],
        "the replacement is what the store holds, not an addendum beside it"
    );

    // And an alias carrying the separator is refused before anything moves.
    let err = store
        .update_entity(
            &id,
            EntityPatch {
                aliases: Some(vec!["one, two".into()]),
                ..Default::default()
            },
        )
        .await
        .expect_err("an alias with a comma in it must be refused");
    assert!(matches!(err, MemoryError::InvalidEntity(_)), "got {err:?}");
    assert_eq!(read_entity(store, &id).await.aliases, vec!["Only This One"]);
}

/// **The guard knows every name an entity answers to.** Someone filed under
/// one name and called another is one entity; a write arriving under the
/// nickname has to hit the same gate a write under the display name does, or
/// a second record gets created under the name the user actually says and
/// the facts split evenly between the two.
pub async fn add_entity_screens_every_name_an_entity_answers_to<M: Memory>(store: &M) {
    let known = EntityId::person("person:contract-many-labelled");
    add(
        store,
        NewEntity {
            aliases: vec!["Contract Nickname Only".into()],
            ..NewEntity::new(known.clone(), "Contract Many-Labelled", "user-named")
        },
    )
    .await;

    let under_the_alias = EntityId::person("person:contract-nickname-only");
    let outcome = store
        .add_entity(NewEntity::new(
            under_the_alias.clone(),
            "Contract Nickname Only",
            "user-named",
        ))
        .await
        .expect("the call itself succeeds; the guard answers in the result");
    let Guarded::Blocked { candidates, .. } = outcome else {
        panic!("a write under a name the entity already answers to must block");
    };
    assert!(
        candidates.iter().any(|m| m.handle == known),
        "the guard must name the entity that wears it: {candidates:?}"
    );
    assert!(
        store
            .list_entities(None)
            .await
            .expect("list")
            .iter()
            .all(|e| e.id != under_the_alias),
        "a blocked add writes nothing"
    );
}

/// **The handle a caller must supply a record at**, before running either
/// spec below — `known()` is what the creation guard screens against, and a
/// fixture that wired no supplied record there would pass on an index
/// nothing could ever miss (rule 234). The handle is the real shipped
/// view's, on the fictional roster already — the same one the review's
/// own example named.
pub const SUPPLIED_VIEW_FOR_THE_GUARD_SPECS: &str = "view:loops";

/// 🚨 **A whole-record provision at an address the store already holds is
/// refused, not silently honoured.**
///
/// `Supplies::Record`'s own contract is a record the store holds NOTHING of.
/// An address a real row already occupies breaks that contract before a
/// caller ever asks anything: every existence check that reads what the
/// build supplies (`list_entities` among them, rule 234) would see the
/// address as already provisioned and never learn the real row needs
/// creating or re-creating — silent, and for a kind whose row is also its
/// mailbox, unrecoverable from inside the running instance.
///
/// **Paired in the same read**: the refusal alone proves nothing if it also
/// destroys the very row it is protecting — so after the refusal, the real
/// entity must still be exactly what `add` wrote, untouched by the check
/// that caught the collision.
pub async fn a_supplied_record_colliding_with_a_stored_row_is_refused<M: Memory>(store: &M) {
    let real = EntityId("thing:contract-provision-collision".into());
    add(
        store,
        NewEntity::new(real.clone(), "Really Stored", "contract-fixture"),
    )
    .await;

    let colliding =
        crate::memory::owned::Provisions::new(vec![crate::memory::owned::Provision::record(
            Entity {
                id: real.clone(),
                kind: EntityKind::THING,
                name: "Shipped By Mistake".into(),
                aliases: Vec::new(),
                source: "jojobot".into(),
                crm: None,
                parent: None,
                boot: Boot::default(),
                merged_into: None,
                badge: None,
                archived: None,
            },
            std::collections::BTreeMap::new(),
        )]);

    let refused = crate::memory::owned::guard_supplied_records(store, &colliding)
        .await
        .expect_err("a whole record at an address the store already holds must be refused");
    assert!(
        matches!(
            &refused,
            MemoryError::SuppliedRecordCollidesWithStoredRow { attempted }
                if attempted == &real.to_string()
        ),
        "the refusal must name the colliding address: {refused:?}",
    );

    let still_there = store
        .list_entities(None)
        .await
        .expect("list_entities should succeed")
        .into_iter()
        .find(|e| e.id == real)
        .expect("the real row must survive the check that caught the collision");
    assert_eq!(
        still_there.name, "Really Stored",
        "the refusal must not have touched the real row: {still_there:?}",
    );
}

/// 🚨 **A claim written on a record the build supplies reads back.**
///
/// The write path already lets one land: the existence gate reads what the
/// build supplies, so `capture` on a supplied handle succeeds and hands
/// back an address. **The read path gated on the ROWS alone**, and a store
/// holding no row for that handle answered every read as if the claim did
/// not exist — so a caller was told the write landed and could then reach
/// it by no route at all.
///
/// ⛔️ **An empty answer is not the repair.** The rows are really there;
/// saying nobody ever wrote them turns a loud wrong answer into a quiet
/// one. **The gate reads rows plus what the build supplies, a stored row
/// winning** — the same set the write path already asks (rule 234).
///
/// **Every read that faulted, in one case**, because they fault for one
/// reason and a case covering one of them says nothing about the rest.
pub async fn a_claim_on_a_supplied_record_reads_back<M: Memory>(store: &M) {
    let shipped = EntityId(SUPPLIED_VIEW_FOR_THE_GUARD_SPECS.into());
    let written = store
        .capture(NewFact {
            fields: [("asks".to_string(), "overdue".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                shipped.clone(),
                "the operator narrowed this one",
                date(2026, 4, 18),
            )
        })
        .await
        .expect("a claim on a supplied record is a write the gate allows")
        .written()
        .expect("nothing blocks it");

    let recalled = store.recall(&shipped).await.expect("recall answers");
    assert!(
        recalled.iter().any(|f| f.id == written.id),
        "the claim that was just written is not in the read: {recalled:?}",
    );
    assert_eq!(
        store
            .fields(&shipped)
            .await
            .expect("the keys read")
            .get("asks"),
        Some(&"overdue".to_string()),
        "the key the write put on it is not in what the thing holds",
    );
    assert_eq!(
        store
            .history(&shipped, "asks")
            .await
            .expect("the writes behind the key read")
            .len(),
        1,
        "the write behind the key is unreachable",
    );
    assert_eq!(
        store
            .claim_history(&written.address())
            .await
            .expect("the claim's own chain reads")
            .len(),
        1,
        "the claim's chain is unreachable",
    );
    assert!(
        store
            .claim_histories(&shipped)
            .await
            .expect("the chains read")
            .contains_key(&written.id),
        "the claim is missing from the chains of the thing it is filed on",
    );

    // **And a handle nobody has is still a miss**, which is what the gate
    // is for: widening it must not turn every typo into an empty page.
    assert!(
        matches!(
            store
                .recall(&EntityId("view:contract-no-such-view".into()))
                .await,
            Err(MemoryError::UnknownEntity { .. }),
        ),
        "a handle nobody has stopped being a miss",
    );
}

/// 🚨 **Every entity read either answers for a record the build supplies,
/// or says here why it must not.**
///
/// Which set a read consults is a per-method decision and nothing counts
/// the methods. A read added after the rule was written inherits whichever
/// half its author copied, and no case asks. **This is the count**: one
/// place naming every read on the port, so a new one is a line somebody has
/// to write rather than a behaviour nobody notices.
///
/// **Three groups, and which group a read is in is the content of the
/// case.**
///
/// * **Gated on what EXISTS** — the rows plus what the build supplies (rule
///   234). `recall`, `fields`, `history`, `claim_history`,
///   `claim_histories` and `backing`. These answer for a supplied record,
///   and a handle nobody has is still a miss.
/// * **Rows only, deliberately.** `list_entities` is the base the layer
///   above extends, so a store resolving supplied records here would
///   resolve them twice; `children` and `scan_entity` derive from it and
///   inherit that. What they answer for a supplied record is the layer
///   above's claim and is asked where that layer is wired.
/// * **Not a lookup at all.** `built_on` and `referring_to` select claims
///   by a pointer rather than resolving a handle, so a handle nobody has
///   selects nothing. **An empty answer is right here and a miss would be
///   the fault.**
///
/// ⛔️ **The miss half is what carries this case.** Asking only that the
/// reads answer passes on a build where every read answers everything,
/// which is exactly what a read with no gate of its own does.
pub async fn every_entity_read_answers_for_a_supplied_record<M: Memory>(store: &M) {
    let shipped = EntityId(SUPPLIED_VIEW_FOR_THE_GUARD_SPECS.into());
    let nobodys = EntityId("view:contract-no-such-view".into());
    let written = store
        .capture(NewFact {
            fields: [("holds".to_string(), "the operator's own".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                shipped.clone(),
                "the operator wrote on the record the build ships",
                date(2026, 5, 11),
            )
        })
        .await
        .expect("a claim on a supplied record is a write the gate allows")
        .written()
        .expect("nothing blocks it");

    // **`backing` is the read this case was written for.** It says which
    // claim stands behind each key, and the port derives it from `fields`
    // and `history` — so a store that overrides it owes the gate they keep,
    // and one that answers without asking tells a caller the handle is fine
    // while every read beside it says it is not.
    let backed = store.backing(&shipped).await.expect("the backing reads");
    assert!(
        backed.contains_key("holds"),
        "the key written on a supplied record has no claim behind it: {backed:?}",
    );

    // ── a handle nobody has is still a miss, on every gated read ────────
    //
    // ⚠️ **Named one at a time on purpose.** Each read carries its own
    // gate, so they fault independently: a case asking this of one of them
    // reports the others as covered and measures nothing about them.
    assert!(
        matches!(
            store.recall(&nobodys).await,
            Err(MemoryError::UnknownEntity { .. })
        ),
        "recall stopped missing a handle nobody has",
    );
    assert!(
        matches!(
            store.fields(&nobodys).await,
            Err(MemoryError::UnknownEntity { .. })
        ),
        "fields stopped missing a handle nobody has",
    );
    assert!(
        matches!(
            store.history(&nobodys, "holds").await,
            Err(MemoryError::UnknownEntity { .. })
        ),
        "history stopped missing a handle nobody has",
    );
    assert!(
        matches!(
            store
                .claim_history(&FactAddress::new(nobodys.clone(), written.id.clone()))
                .await,
            Err(MemoryError::UnknownEntity { .. })
        ),
        "claim_history stopped missing a handle nobody has",
    );
    assert!(
        matches!(
            store.claim_histories(&nobodys).await,
            Err(MemoryError::UnknownEntity { .. })
        ),
        "claim_histories stopped missing a handle nobody has",
    );
    assert!(
        matches!(
            store.backing(&nobodys).await,
            Err(MemoryError::UnknownEntity { .. })
        ),
        "backing answers for a handle nobody has, while every read beside it misses",
    );

    // ── the two reads that are a selection rather than a lookup ────────
    //
    // **Each is asked twice in one read**, because an emptiness on its own
    // holds identically on a build where the read returns nothing at all.
    // The positive is that the selection reaches a supplied record; the
    // negative is that a handle nobody has selects nothing rather than
    // missing.
    let pointer = EntityId("person:contract-supplied-pointer".into());
    add(
        store,
        NewEntity::new(pointer.clone(), "Contract Pointer", "contract-fixture"),
    )
    .await;
    let points_at_it = capture(
        store,
        NewFact {
            fields: [("asks".to_string(), shipped.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(pointer.clone(), "runs the shipped one", date(2026, 5, 12))
        },
    )
    .await;
    let stands_on_it = capture(
        store,
        NewFact {
            derived_from: Some(written.address()),
            ..NewFact::about(
                pointer.clone(),
                "so the shipped one is narrowed",
                date(2026, 5, 13),
            )
        },
    )
    .await;

    let standing_on = store
        .built_on(&written.address())
        .await
        .expect("the lineage of a claim on a supplied record reads");
    assert!(
        standing_on.iter().any(|f| f.id == stands_on_it.id),
        "a claim built on one written on a supplied record is unreachable from it",
    );
    assert!(
        store
            .built_on(&FactAddress::new(nobodys.clone(), written.id.clone()))
            .await
            .expect("a lineage read is a selection on a pointer, never a lookup")
            .is_empty(),
        "a lineage read of a handle nobody has selected something",
    );

    let pointing = store
        .referring_to(&shipped)
        .await
        .expect("what points at a supplied record reads");
    assert!(
        pointing.iter().any(|f| f.id == points_at_it.id),
        "a key holding a supplied record's handle does not answer from the far end",
    );
    assert!(
        store
            .referring_to(&nobodys)
            .await
            .expect("a pointer read is a selection, never a lookup")
            .is_empty(),
        "a pointer read of a handle nobody has selected something",
    );
}

/// **Renaming a record the build supplies is refused, never a silent
/// no-op — and never a claim that it moved to itself.**
/// `rename_entity`'s existence check reads stored rows only,
/// deliberately excluding what the build supplies — a comment beside it
/// names why: resolving that check through `known()` would let a
/// supplied handle pass it and fall through every guard below, while
/// the mutation itself only ever touches stored rows. Nothing would
/// move, and the caller would still be told `Guarded::Written`.
///
/// **Paired with a stored rename that still works**, or a store that
/// refuses every rename outright would pass this case identically to
/// the one it exists to catch.
pub async fn a_rename_of_a_supplied_handle_is_refused_not_a_silent_no_op<M: Memory>(store: &M) {
    let shipped = EntityId(SUPPLIED_VIEW_FOR_THE_GUARD_SPECS.into());
    let target = EntityId("view:contract-supplied-rename-target".into());

    let err = store
        .rename_entity(&shipped, &target, None, date(2026, 6, 10), None)
        .await
        .expect_err("a rename of a build-supplied handle must be refused, not silently accepted");
    assert!(
        matches!(&err, MemoryError::SuppliedHandle { attempted } if attempted == &shipped.to_string()),
        "a supplied handle's rename must say it is a supplied record with no row to move, \
         never a bare miss, and never a claim that it moved to itself: {err:?}",
    );
    assert!(
        store
            .list_entities(None)
            .await
            .expect("list_entities should succeed")
            .iter()
            .all(|e| e.id != target),
        "the destination handle must not exist: a refused rename wrote nothing",
    );

    // A genuinely stored thing still renames — the case this one must
    // be told apart from is a store that refuses every rename outright.
    let was = EntityId("thing:contract-supplied-rename-stored".into());
    add(
        store,
        NewEntity::new(was.clone(), "Stored, Not Supplied", "contract-fixture"),
    )
    .await;
    let now = EntityId("thing:contract-supplied-rename-stored-now".into());
    store
        .rename_entity(&was, &now, None, date(2026, 6, 10), None)
        .await
        .expect("a stored entity's rename should succeed")
        .written()
        .expect("nothing collides with the destination");
    assert!(
        store
            .list_entities(None)
            .await
            .expect("list_entities should succeed")
            .iter()
            .any(|e| e.id == now),
        "a genuinely stored rename must land",
    );
}

/// **Archiving persists the reason and the moment, and survives a read taken
/// after the write** — the storage half of the caller-facing bar, which
/// proves the same thing behaviourally through the served surface. Both the
/// fake and the real store answer for this, through [`run_all`].
///
/// **Paired with a live entity, untouched by the same write** — the negative
/// alone would pass on a store that answered `archived` for everything.
pub async fn archive_entity_persists_the_reason_and_the_moment<M: Memory>(store: &M) {
    let target = add(
        store,
        NewEntity::new(
            EntityId("person:contract-marked-target".into()),
            "Marked Target",
            "contract-fixture",
        ),
    )
    .await;
    let untouched = add(
        store,
        NewEntity::new(
            EntityId("person:contract-marked-untouched".into()),
            "Marked Untouched",
            "contract-fixture",
        ),
    )
    .await;

    let before = jiff::Timestamp::now();
    let written = store
        .archive_entity(&target.id, "a mistaken write")
        .await
        .expect("archive_entity should succeed");
    let after = jiff::Timestamp::now();

    let state = written
        .archived
        .as_ref()
        .expect("archive_entity must set archived on what it returns");
    assert_eq!(state.reason, "a mistaken write", "{state:?}");
    assert!(
        state.at >= before && state.at <= after,
        "the moment must be stamped by the store, between the call's own bounds: {state:?}",
    );

    // Read-back, through the raw port rather than the answer the write
    // handed back — the bar every other write on this port holds to.
    let index = store
        .list_entities(None)
        .await
        .expect("list_entities should succeed");
    let reread = index
        .iter()
        .find(|e| e.id == target.id)
        .expect("the archived entity must still be listed by the raw port");
    assert_eq!(
        reread.archived.as_ref(),
        Some(state),
        "archiving must survive a read taken after the write: {reread:?}",
    );
    let still_live = index
        .iter()
        .find(|e| e.id == untouched.id)
        .expect("the untouched entity must still be listed");
    assert!(
        still_live.archived.is_none(),
        "archiving one entity must not touch another: {still_live:?}",
    );
}

/// **A second archive is refused, never a silent overwrite** — the same
/// one-way rule [`check_retractable`] holds for a claim jojobot is already
/// holding archived.
///
/// **Paired with a fresh archive, which still lands** — the negative alone
/// would pass on a store that refused every archive outright.
pub async fn a_second_archive_is_refused_not_overwritten<M: Memory>(store: &M) {
    let target = add(
        store,
        NewEntity::new(
            EntityId("person:contract-marked-twice".into()),
            "Marked Twice",
            "contract-fixture",
        ),
    )
    .await;
    let first = store
        .archive_entity(&target.id, "a mistaken write")
        .await
        .expect("the first archive should succeed")
        .archived
        .expect("the first archive must set archived");

    let err = store
        .archive_entity(&target.id, "somebody not relevant at all")
        .await
        .expect_err("a second archive must be refused, not silently accepted");
    assert!(
        matches!(&err, MemoryError::AlreadyArchived { attempted } if attempted == &target.id.to_string()),
        "a second archive must say the entity is already archived: {err:?}",
    );

    let index = store
        .list_entities(None)
        .await
        .expect("list_entities should succeed");
    let reread = index
        .iter()
        .find(|e| e.id == target.id)
        .expect("the entity must still be listed");
    assert_eq!(
        reread.archived.as_ref(),
        Some(&first),
        "a refused second archive must leave the first reason and moment untouched: {reread:?}",
    );
}

/// **Naming a build-supplied record has no row to mark, and the refusal says
/// so rather than reading as a bare miss** — the same shape
/// `rename_entity`'s own supplied-handle refusal takes.
///
/// **Paired with a genuinely stored entity, which still archives** — a store
/// that refused every archive outright would pass the negative alone.
pub async fn an_archive_of_a_supplied_handle_is_refused_not_a_silent_no_op<M: Memory>(store: &M) {
    let shipped = EntityId(SUPPLIED_VIEW_FOR_THE_GUARD_SPECS.into());
    let err = store
        .archive_entity(&shipped, "a mistaken write")
        .await
        .expect_err("archiving a build-supplied handle must be refused, not silently accepted");
    assert!(
        matches!(&err, MemoryError::SuppliedHandle { attempted } if attempted == &shipped.to_string()),
        "a supplied handle's archive must say it is a supplied record with no row to mark, never \
         a bare miss: {err:?}",
    );

    let stored = add(
        store,
        NewEntity::new(
            EntityId("person:contract-marked-supplied-stored".into()),
            "Marked Supplied Stored",
            "contract-fixture",
        ),
    )
    .await;
    let written = store
        .archive_entity(&stored.id, "a mistaken write")
        .await
        .expect("a genuinely stored entity's archive should succeed");
    assert!(
        written.archived.is_some(),
        "a genuinely stored archive must land",
    );
}

/// **Naming a build-supplied record on either side of a fold is refused,
/// never a bare miss** — `merge`'s existence check reads stored rows only,
/// deliberately excluding what the build supplies, exactly as
/// `rename_entity`'s does; a supplied record's handle reads, lists and
/// searches as existing, so `UnknownEntity` there would be a lie the caller
/// has no way to catch.
///
/// **Paired with a genuinely unknown handle, which still misses as such** —
/// or a store that answered `SuppliedHandle` for every not-found handle
/// would pass this case identically to the one it exists to catch (rule
/// 234).
pub async fn a_merge_naming_a_supplied_handle_is_refused_not_a_silent_no_op<M: Memory>(store: &M) {
    let shipped = EntityId(SUPPLIED_VIEW_FOR_THE_GUARD_SPECS.into());
    let stored = EntityId("thing:contract-supplied-merge-stored".into());
    add(
        store,
        NewEntity::new(
            stored.clone(),
            "Merge Stored, Not Supplied",
            "contract-fixture",
        ),
    )
    .await;

    // The supplied record as the duplicate side.
    let as_folded = store
        .merge(&shipped, &stored, None, date(2026, 6, 13))
        .await
        .expect_err("folding a build-supplied record away must be refused");
    assert!(
        matches!(&as_folded, MemoryError::SuppliedHandle { attempted } if attempted == &shipped.to_string()),
        "a supplied record named as the duplicate must say it is a supplied record with no \
         row to move, never a bare miss: {as_folded:?}",
    );

    // The supplied record as the survivor side.
    let as_survivor = store
        .merge(&stored, &shipped, None, date(2026, 6, 13))
        .await
        .expect_err("folding a stored record into a build-supplied one must be refused");
    assert!(
        matches!(&as_survivor, MemoryError::SuppliedHandle { attempted } if attempted == &shipped.to_string()),
        "a supplied record named as the survivor must say the same: {as_survivor:?}",
    );

    assert!(
        store
            .list_entities(None)
            .await
            .expect("list_entities should succeed")
            .iter()
            .all(|e| e.id != stored || e.merged_into.is_none()),
        "a refused fold must write nothing: the stored side must not end up folded away",
    );

    // A genuinely unknown handle still misses as such — the case this one
    // must be told apart from is a store that answers SuppliedHandle for
    // every miss.
    let nobody = EntityId("thing:contract-supplied-merge-nobody".into());
    let unknown = store
        .merge(&nobody, &stored, None, date(2026, 6, 13))
        .await
        .expect_err("a handle nothing ever held must still miss");
    assert!(
        matches!(&unknown, MemoryError::UnknownEntity { attempted, .. } if attempted == &nobody.to_string()),
        "a genuinely unknown handle must miss as such, not as a supplied one: {unknown:?}",
    );
}

/// **A genuine move still says where the thing went; a handle nothing
/// ever held still misses — from `rename_entity` itself**, not only
/// the pure resolver [`super::resolve_handle`] already covers.
///
/// Paired so the two misses cannot be confused: a store that answered
/// `HandleMoved` for every not-found handle would still pass the
/// second half alone, and one that answered `UnknownEntity` for every
/// not-found handle would still pass the first half alone.
pub async fn a_second_rename_of_a_stale_handle_reports_where_it_went<M: Memory>(store: &M) {
    let was = EntityId("thing:contract-double-rename-was".into());
    add(
        store,
        NewEntity::new(was.clone(), "Double Rename", "contract-fixture"),
    )
    .await;
    let now = EntityId("work:contract-double-rename-now".into());
    store
        .rename_entity(&was, &now, None, date(2026, 6, 11), None)
        .await
        .expect("the first rename lands")
        .written()
        .expect("nothing collides with the destination");

    let elsewhere = EntityId("person:contract-double-rename-elsewhere".into());
    let moved = store
        .rename_entity(&was, &elsewhere, None, date(2026, 6, 12), None)
        .await
        .expect_err("a stale handle must not be renamed as if it still had a row");
    assert!(
        matches!(&moved, MemoryError::HandleMoved { attempted, now: reported }
            if attempted == &was.to_string() && reported == &now.to_string()),
        "a genuinely moved handle must say where it went: {moved:?}",
    );

    let never = EntityId("person:contract-rename-attempt-never-existed".into());
    let missed = store
        .rename_entity(&never, &elsewhere, None, date(2026, 6, 12), None)
        .await
        .expect_err("a handle nothing ever held must miss");
    assert!(
        matches!(&missed, MemoryError::UnknownEntity { attempted, .. }
            if attempted == &never.to_string()),
        "a handle nothing ever held must be a bare miss, not a move: {missed:?}",
    );
}

/// An entity write with a malformed field is refused outright — a name that
/// could break out of its frontmatter line never reaches the store.
pub async fn malformed_entity_fields_are_rejected<M: Memory>(store: &M) {
    let id = EntityId::person("person:contract-injector");
    for (name, source, crm) in [
        ("", "user-named", None),
        ("ok", "", None),
        ("bad\nid: person:someone-else", "user-named", None),
        ("bad ```", "user-named", None),
        // A cross-link must stay one token on its frontmatter line, so
        // whitespace, a comma and a backtick are all refused whatever the
        // task layer's grammar is.
        ("ok", "user-named", Some("card 874")),
        ("ok", "user-named", Some("ENG-421,ENG-422")),
        ("ok", "user-named", Some("ENG-`421`")),
        ("ok", "user-named", Some("")),
    ] {
        let err = store
            .add_entity(NewEntity {
                crm: crm.map(str::to_string),
                ..NewEntity::new(id.clone(), name, source)
            })
            .await
            .expect_err("a malformed entity field must be rejected");
        assert!(
            matches!(err, MemoryError::InvalidEntity(_)),
            "expected InvalidEntity for {name:?}/{source:?}/{crm:?}, got {err:?}"
        );
    }
}

/// **The cross-link takes the task layer's own grammar.** `crm` points at
/// this entity in whatever system holds the operator's tasks, and that
/// system decides how it addresses things: `card:874` in one, `ENG-421` in
/// another. A validator that accepted only one of those refused the link
/// outright to every other layer, which left the entity with no way to
/// record it at all.
pub async fn a_cross_link_takes_the_task_layers_own_grammar<M: Memory>(store: &M) {
    let id = EntityId::person("person:contract-crosslink");
    add(
        store,
        NewEntity {
            crm: Some("ENG-421".into()),
            ..NewEntity::new(id.clone(), "Beta", "user-named")
        },
    )
    .await;
    let seen = read_entity(store, &id).await;
    assert_eq!(seen.crm.as_deref(), Some("ENG-421"));
}

/// **A frontmatter field at the validator's limit survives the store.**
///
/// `source` and `crm` are screened by one rule — one plain line, at most
/// two hundred characters — and a column narrower than that rule refuses a
/// value the domain admits. The write is already past every check jojobot
/// makes when the store answers, so nothing above the port sees it coming,
/// and a double cannot answer for it: a double keeps whatever bytes it is
/// handed, whatever a column would have said.
///
/// The two fields are written together and asserted apart, because they
/// are two columns and a widening that reaches one leaves the other
/// exactly as it was.
pub async fn a_field_at_the_validators_limit_survives_storage<M: Memory>(store: &M) {
    let id = EntityId::person("person:contract-brimful");
    let source = format!("{:-<200}", "contract-source-");
    let crm = format!("{:0<200}", "card:");
    assert_eq!(
        (source.chars().count(), crm.chars().count()),
        (200, 200),
        "this case is about the limit only if it writes the limit",
    );

    add(
        store,
        NewEntity {
            crm: Some(crm.clone()),
            ..NewEntity::new(id.clone(), "Omicron", source.clone())
        },
    )
    .await;

    let seen = read_entity(store, &id).await;
    assert_eq!(
        seen.source, source,
        "the source the validator admits comes back whole",
    );
    assert_eq!(
        seen.crm.as_deref(),
        Some(crm.as_str()),
        "…and so does the cross-link, which is a column of its own",
    );
}

// --- declared types ------------------------------------------------------

/// Declare a type the store is expected to keep.
async fn declare<M: Memory>(store: &M, declared: DeclaredType) -> DeclaredType {
    let name = declared.name.clone();
    store
        .declare_type(declared)
        .await
        .unwrap_or_else(|e| panic!("declaring '{name}' should succeed: {e}"))
}

/// One type out of the store's own listing.
async fn read_type<M: Memory>(store: &M, name: &str) -> DeclaredType {
    store
        .declared_types()
        .await
        .expect("declared_types should succeed")
        .into_iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("declared_types must return '{name}'"))
}

/// **A declared type reads back, complete and in the order it was
/// declared.**
///
/// The order is a claim and not a convenience: an answer names the keys a
/// record holds and lacks in the type's own order, so a store that sorted
/// them by name would replace an order somebody chose with one nobody did.
/// It is asserted with keys whose declared order is NOT their alphabetical
/// order, or a sorting store passes this unchanged.
pub async fn a_declared_type_reads_back<M: Memory>(store: &M) {
    let declared = DeclaredType::new(
        "contract-tenancy",
        vec![
            Field::required("starts", ValueType::Date),
            Field::required("rooms", ValueType::Number),
            Field::required("building", ValueType::Reference),
            Field::required("active", ValueType::Boolean),
        ],
    );
    let written = declare(store, declared.clone()).await;
    assert_eq!(written, declared, "the store returns what it stored");
    assert_eq!(
        read_type(store, "contract-tenancy").await,
        declared,
        "…and a later read returns it unchanged, keys and order and all",
    );

    // The negative the read rests on: nothing invents a type nobody
    // declared. Without it the assertions above hold on a store that
    // answers every question with every type it can think of.
    let known = store
        .declared_types()
        .await
        .expect("declared_types should succeed");
    assert!(
        !known.iter().any(|t| t.name == "contract-never-declared"),
        "a type nobody declared is not in the store: {known:?}",
    );
}

/// **Declaring again replaces the keys whole.**
///
/// A type is the set of keys it names now. One that accumulated every key
/// it had ever named would describe no record at all, and every match
/// against it would report keys the writer had already dropped as lacking.
pub async fn declaring_a_type_again_replaces_its_keys<M: Memory>(store: &M) {
    declare(
        store,
        DeclaredType::new(
            "contract-parcel",
            vec![
                Field::required("weight", ValueType::Number),
                Field::required("sender", ValueType::Reference),
            ],
        ),
    )
    .await;
    let second = DeclaredType::new(
        "contract-parcel",
        vec![
            Field::required("weight", ValueType::Number),
            Field::required("arrives", ValueType::Date),
        ],
    );
    declare(store, second.clone()).await;

    assert_eq!(
        read_type(store, "contract-parcel").await,
        second,
        "the second declaration is what the type is now",
    );
    assert_eq!(
        store
            .declared_types()
            .await
            .expect("declared_types should succeed")
            .iter()
            .filter(|t| t.name == "contract-parcel")
            .count(),
        1,
        "and it replaced the first rather than standing beside it",
    );
}

/// **A type with no keys is refused, and the store keeps nothing.**
///
/// A type arrives complete with its fields. A bare name is a type nobody
/// can query by, and it would have to be configured into usefulness before
/// it did anything.
///
/// Both halves: the refusal, and that the refusal wrote nothing. A store
/// that errored after inserting the name would pass the first alone.
pub async fn a_type_with_no_keys_is_refused_and_writes_nothing<M: Memory>(store: &M) {
    let refused = store
        .declare_type(DeclaredType::new("contract-empty", vec![]))
        .await;
    assert!(
        matches!(refused, Err(MemoryError::InvalidType(_))),
        "a type is the keys it names: {refused:?}",
    );
    let known = store
        .declared_types()
        .await
        .expect("declared_types should succeed");
    assert!(
        !known.iter().any(|t| t.name == "contract-empty"),
        "a refused declaration leaves nothing behind: {known:?}",
    );

    // The positive it rests on: the same store takes a declaration that
    // does name a key, so the absence above is the refusal and not a store
    // that declares nothing at all.
    declare(
        store,
        DeclaredType::new(
            "contract-not-empty",
            vec![Field::required("x", ValueType::Text)],
        ),
    )
    .await;
    assert_eq!(read_type(store, "contract-not-empty").await.fields.len(), 1);
}

/// **A type that ships with the software is closed to callers, and the
/// store is what closes it.**
///
/// Three claims in one case, because each is worthless without the others.
/// The origin survives being written and read back — a store that dropped
/// it would leave every shipped type looking like a caller's. A caller's
/// declaration over that name is refused and changes nothing. And the same
/// caller's own type still replaces on redeclare, so what was refused is
/// the origin and not the act of redeclaring.
pub async fn a_shipped_type_refuses_a_callers_redeclaration<M: Memory>(store: &M) {
    let shipped = DeclaredType::shipped(
        "contract-rota",
        vec![
            Field::required("starts", ValueType::Date),
            Field::required("cover", ValueType::Reference),
        ],
    );
    declare(store, shipped.clone()).await;
    assert_eq!(
        read_type(store, "contract-rota").await.origin,
        Origin::Shipped,
        "where the type came from survives the store",
    );

    let refused = store
        .declare_type(DeclaredType::new(
            "contract-rota",
            vec![Field::required("starts", ValueType::Date)],
        ))
        .await;
    assert!(
        matches!(refused, Err(MemoryError::ShippedType { .. })),
        "a caller cannot declare over a type the software ships: {refused:?}",
    );
    assert_eq!(
        read_type(store, "contract-rota").await,
        shipped,
        "…and the refused declaration left the shipped type exactly as it was",
    );

    // The positive the refusal rests on: redeclaring is not what was
    // refused. Without this the case above passes on a store that refuses
    // every second declaration of any name at all.
    declare(
        store,
        DeclaredType::new(
            "contract-shift",
            vec![Field::required("starts", ValueType::Date)],
        ),
    )
    .await;
    let replaced = DeclaredType::new(
        "contract-shift",
        vec![Field::required("cover", ValueType::Text)],
    );
    declare(store, replaced.clone()).await;
    assert_eq!(
        read_type(store, "contract-shift").await,
        replaced,
        "a caller's own type replaces on redeclare, origin and all",
    );
    assert_eq!(
        read_type(store, "contract-shift").await.origin,
        Origin::Declared,
        "and a caller's type reads back as a caller's",
    );
}

/// **Two types may name one key and mean their own thing by it — through
/// the store.**
///
/// Keys are scoped by the type that names them and registered nowhere, so
/// there is no list of legal keys anywhere and nothing to keep in step. A
/// store keying its rows on the key name alone would lose one of these, and
/// nothing above it could tell.
pub async fn two_stored_types_may_name_one_key<M: Memory>(store: &M) {
    declare(
        store,
        DeclaredType::new(
            "contract-seating",
            vec![Field::required("seats", ValueType::Text)],
        ),
    )
    .await;
    declare(
        store,
        DeclaredType::new(
            "contract-booking",
            vec![Field::required("seats", ValueType::Number)],
        ),
    )
    .await;

    assert_eq!(
        read_type(store, "contract-seating").await.fields[0].holds,
        ValueType::Text,
    );
    assert_eq!(
        read_type(store, "contract-booking").await.fields[0].holds,
        ValueType::Number,
        "one key name, two types, and each keeps what it declared",
    );
}

/// **A type read back out of the store still matches a record nobody
/// declared.**
///
/// The journey, rather than the function: the declaration goes into the
/// store, comes back out, and matches a loose bag of keys that was never
/// told which type it was. This is what makes matching structural in the
/// built system rather than in one pure function — a store that mangled a
/// key name or a value type would leave the domain's own cases green and
/// every real query wrong.
pub async fn a_stored_type_matches_a_record_that_never_declared_it<M: Memory>(store: &M) {
    declare(
        store,
        DeclaredType::new(
            "contract-shipment",
            vec![
                Field::required("weight", ValueType::Number),
                Field::required("arrives", ValueType::Date),
            ],
        ),
    )
    .await;
    let stored = read_type(store, "contract-shipment").await;

    let loose: std::collections::BTreeMap<String, String> = [("weight", "12")]
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    let found = stored
        .matched_by(&loose)
        .expect("a record carrying one of the keys answers the type");
    assert_eq!(found.held, vec!["weight"]);
    assert_eq!(
        found.lacking,
        vec!["arrives"],
        "a partial match comes back and names what it lacks: {found:?}",
    );

    // And the negative it rests on, with the same stored type: a record
    // sharing no key is not a match, or "it matched" says nothing.
    let unrelated: std::collections::BTreeMap<String, String> =
        [("mood".to_string(), "curious".to_string())]
            .into_iter()
            .collect();
    assert_eq!(stored.matched_by(&unrelated), None);
}

/// Run the whole contract against one store.
/// **A kind selects its objects, and each one's page comes back whole.**
///
/// The journey rather than the function: the prose goes into the store,
/// and a query that named a KIND — never a handle — brings it back. A
/// charter is prose, so this is the shape behind "the objects of a kind,
/// with their prose", and nothing on this path knows what a charter is.
///
/// It asserts containment rather than the whole list: the contract runs
/// every case against one store, so other cases' entities are legitimately
/// there. The negative it rests on is a different kind, which must NOT be
/// in a kind-scoped answer however many objects share the store.
pub async fn a_graph_query_selects_a_kind_and_returns_its_prose<M: Memory>(store: &M) {
    let one = EntityId("bot:contract-graph-one".into());
    let two = EntityId("bot:contract-graph-two".into());
    let outsider = EntityId::person("person:contract-graph-outsider");
    for id in [&one, &two, &outsider] {
        ensure(store, id).await;
    }
    // Several paragraphs, because prose comes back WHOLE and a store that
    // kept the first line would satisfy a one-line fixture.
    let charter = "Keeps the roster.\n\nHard line: never writes to the ledger.\n\nWorks in \
                   the yard.";
    store
        .set_prose(&one, charter)
        .await
        .expect("set_prose should succeed");
    store
        .set_prose(&outsider, "Not a bot, and holds a page anyway.")
        .await
        .expect("set_prose should succeed");

    let found = graph::walk(
        store,
        &graph::GraphQuery {
            select: graph::Selection {
                kind: Some(EntityKind::BOT),
                ..graph::Selection::default()
            },
            include: graph::Include {
                facts: false,
                prose: true,
                stood_for: false,
            },
            follow: None,
            history: None,
        },
    )
    .await
    .expect("a kind is a selection")
    .objects;

    let page = |id: &EntityId| {
        found
            .iter()
            .find(|o| &o.entity.id == id)
            .map(|o| o.prose.clone().unwrap_or_default())
    };
    assert_eq!(
        page(&one).as_deref(),
        Some(charter),
        "the page comes back whole, through a query that named only a kind",
    );
    assert_eq!(
        page(&two).as_deref(),
        Some(""),
        "a bot with no page comes back with an empty one, not missing from the answer",
    );
    assert_eq!(
        page(&outsider),
        None,
        "and the kind filter holds: a person is not in an answer about bots",
    );
}

/// **A key's VALUE selects, over a value that survived storage — and the
/// edge beside it is walkable from either end.**
///
/// Two claims about the store in one case, because they are one write.
/// The value carries the punctuation battery for the reason
/// [`an_event_survives_capture`] does: a markdown store rewrites markdown,
/// so a filter comparing against what the caller SENT will miss what the
/// store KEPT, and a fake that stores bytes verbatim never shows it.
///
/// The negative is the same key with the other value, which must select
/// the other record — a build ignoring the value passes the positive and
/// fails here.
pub async fn a_graph_query_filters_on_a_stored_value_and_walks_an_edge<M: Memory>(store: &M) {
    let gathering = EntityId("event:contract-graph-gathering".into());
    let coming = EntityId::person("person:contract-graph-coming");
    let staying = EntityId::person("person:contract-graph-staying");
    ensure(store, &gathering).await;

    let yes = "yes = certain, ~confirmed~ 100% ünïcode";
    let reply = |who: &EntityId, answer: &str, said: &str| NewFact {
        edge: Some(Edge::new(EdgeShape::Attendance, gathering.clone())),
        fields: [("answer".to_string(), answer.to_string())]
            .into_iter()
            .collect(),
        refs: Vec::new(),
        ..NewFact::about(who.clone(), said, date(2026, 8, 10))
    };
    capture(store, reply(&coming, yes, "will be there")).await;
    capture(store, reply(&staying, "no", "cannot make it")).await;

    let selected = |answer: &str| {
        let answer = answer.to_string();
        async move {
            let found = graph::walk(
                store,
                &graph::GraphQuery {
                    select: graph::Selection {
                        fields: vec![graph::FieldFilter::holding("answer", &answer)],
                        ..graph::Selection::default()
                    },
                    ..graph::GraphQuery::default()
                },
            )
            .await
            .expect("a key filter is a selection")
            .objects;
            found
                .iter()
                .map(|o| o.entity.id.clone())
                .collect::<Vec<_>>()
        }
    };
    let confirmed = selected(yes).await;
    assert!(
        confirmed.contains(&coming),
        "the value the store KEPT is what the filter must match: {confirmed:?}",
    );
    assert!(
        !confirmed.contains(&staying),
        "and the other answer is not in it: {confirmed:?}",
    );
    let declined = selected("no").await;
    assert!(
        declined.contains(&staying) && !declined.contains(&coming),
        "the same key with the other value selects the other record: {declined:?}",
    );

    // The edge, walked back from the thing both records point at. The
    // guests were never named in this query: they are reached.
    let walked = graph::walk(
        store,
        &graph::GraphQuery {
            select: graph::Selection {
                subject: Some(gathering.clone()),
                ..graph::Selection::default()
            },
            include: graph::Include::default(),
            follow: Some(graph::Follow {
                along: graph::Along::Edge(EdgeShape::Attendance),
                direction: Some(graph::Direction::In),
                depth: 1,
                keeping: Vec::new(),
                fits_type: None,
            }),
            history: None,
        },
    )
    .await
    .expect("a subject with a walk")
    .objects;
    let reached: Vec<&EntityId> = walked[0].connected.iter().map(|o| &o.entity.id).collect();
    assert!(
        reached.contains(&&coming) && reached.contains(&&staying),
        "both guests hang off the gathering: {reached:?}",
    );
    let brought = walked[0]
        .connected
        .iter()
        .find(|o| o.entity.id == coming)
        .expect("the guest who is coming");
    assert!(
        brought.facts.iter().any(|f| f.content == "will be there"),
        "and a reached object arrives carrying its own records: {brought:?}",
    );
    assert_eq!(
        brought.via,
        Some(graph::Via {
            link: graph::Link::Edge(EdgeShape::Attendance),
            direction: graph::Direction::In,
            retracted: false,
        }),
        "which says how the walk got to it, and that the claim behind it stands",
    );
}

/// 🚨 **A document's id is the entity's badge, not its handle — so it
/// survives a rewrite and is not the name anybody sends.**
///
/// The index evicts a document by its id. While that id WAS the handle, the
/// two were one thing and a rename would have had to move the postings with
/// it. **The badge is what makes them separable**, and this is the read that
/// says the seam is open rather than merely present in a column.
///
/// **Three reads.** A rewrite of the entity leaves the document's id alone;
/// two entities never share one; and the id is not the handle, which is the
/// half that fails on a store where the seam is still collapsed and the
/// other two pass.
pub async fn a_documents_id_is_not_the_handle_and_survives_a_rewrite<M: Memory>(store: &M) {
    let held = EntityId::person("person:contract-stamped");
    let other = EntityId::person("person:contract-inked");
    ensure(store, &held).await;
    ensure(store, &other).await;

    let id_of = |scanned: &[crate::memory::search::DocScan], who: &EntityId| {
        scanned
            .iter()
            .find(|d| d.entity.as_ref().is_some_and(|e| &e.id == who))
            .unwrap_or_else(|| panic!("{who:?} is not in the scan"))
            .doc_id
            .clone()
    };

    let before = store.scan().await.expect("the store scans");
    let badge = id_of(&before, &held);
    assert_ne!(
        badge,
        held.to_string(),
        "the document's id is the handle, so the two cannot come apart and a rename would \
         have to move the postings with it",
    );
    assert_ne!(
        badge,
        id_of(&before, &other),
        "two entities share a document id, so evicting one would evict the other",
    );

    store
        .update_entity(
            &held,
            EntityPatch {
                name: Some("Contract Badged, renamed".into()),
                ..Default::default()
            },
        )
        .await
        .expect("update_entity should succeed")
        .written()
        .expect("the guard waves it through");
    let after = store.scan().await.expect("the store scans");
    assert_eq!(
        id_of(&after, &held),
        badge,
        "the entity was rewritten and its document became a different one, so what the index \
         holds under the old id is now unreachable",
    );
}

/// 🚨 **Is this held as a VALUE anywhere, under a key the caller cannot
/// name — as opposed to sitting in prose.**
///
/// Selecting by a named key exists and reporting one key's values exists.
/// **Neither asks whether a string is a value at all**, and that is what
/// *did somebody record this so it can be asked for later* reduces to: a
/// caller knows what was written and not where it went.
///
/// 🚨 **The prose half is the one that matters.** The same string written
/// into a sentence must NOT match — a claim's wording is not something a
/// later question can be asked of, and an answer that could not tell the
/// two apart would be the breadth verb wearing this one's name.
///
/// **And a string held nowhere comes back empty**, so neither half passes
/// on a store where the selection reaches everything or nothing.
pub async fn a_value_is_found_without_naming_the_key_it_is_under<M: Memory>(store: &M) {
    let filed = EntityId::person("person:contract-filed");
    let spoken = EntityId::person("person:contract-spoken");
    ensure(store, &filed).await;
    ensure(store, &spoken).await;

    let day = "2026-08-11";
    capture(
        store,
        NewFact {
            fields: [("serviced_on".to_string(), day.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(filed.clone(), "the service happened", date(2026, 8, 11))
        },
    )
    .await;
    // The same string, written into the sentence and held under no key.
    capture(
        store,
        NewFact::about(
            spoken.clone(),
            format!("the service happened on {day}").as_str(),
            date(2026, 8, 11),
        ),
    )
    .await;

    let holding = |value: &str| {
        let value = value.to_string();
        async move {
            graph::walk(
                store,
                &graph::GraphQuery {
                    select: graph::Selection {
                        fields: vec![graph::FieldFilter::anywhere(&value)],
                        ..graph::Selection::default()
                    },
                    ..graph::GraphQuery::default()
                },
            )
            .await
            .expect("a value filter is a selection")
            .objects
            .iter()
            .map(|o| o.entity.id.clone())
            .collect::<Vec<_>>()
        }
    };

    let found = holding(day).await;
    assert!(
        found.contains(&filed),
        "a value stored under a key was not found by asking for the value: {found:?}",
    );
    assert!(
        !found.contains(&spoken),
        "the same string sitting in a claim's prose matched, so this answers *written down \
         somewhere* rather than *held as a value*: {found:?}",
    );
    assert!(
        holding("2011-01-01").await.is_empty(),
        "a string nothing holds came back with objects, so the two reads above say nothing",
    );
}

/// 🚨 **A rewrite can take the edge off, and one that does not mention
/// edges leaves it where it is.**
///
/// [`FactStatus`] says a disproved claim is rewritten to the negative
/// truth and stays active. Without a way to take the edge off, *was at the
/// fair* corrected to *was never at the fair* keeps an attendance edge
/// behind a sentence denying it, and the graph goes on answering the
/// question the claim no longer does.
///
/// **Both halves in one case, and the second is the load-bearing one.** A
/// build where every rewrite silently dropped the edge would pass the
/// clearing half and be a far worse defect than the one this fixes — a
/// caller correcting a typo would lose the link and never be told.
///
/// **And the walk, because that is the property a caller wants.** The
/// record read says the cell is empty; only the walk says the graph has
/// stopped answering through it.
pub async fn a_rewrite_can_take_the_edge_off_and_leaves_it_alone_otherwise<M: Memory>(store: &M) {
    let fair = EntityId("event:contract-unlinked-fair".into());
    let went = EntityId::person("person:contract-unlinked-attended");
    let never = EntityId::person("person:contract-unlinked-absent");
    ensure(store, &fair).await;

    let attending = |who: &EntityId| NewFact {
        edge: Some(Edge::new(EdgeShape::Attendance, fair.clone())),
        ..NewFact::about(who.clone(), "was at the fair", date(2026, 8, 10))
    };
    let stays = capture(store, attending(&went)).await;
    let wrong = capture(store, attending(&never)).await;

    // **The half that must not change.** A rewrite naming neither the shape
    // nor the object says nothing about edges, so the edge stands.
    let reworded = edit(
        store,
        &stays.address(),
        FactPatch {
            content: Some("was at the fair, all afternoon".into()),
            ..Default::default()
        },
    )
    .await;
    assert!(
        reworded.edge.is_some(),
        "a rewrite that never mentioned edges took one off: {reworded:?}",
    );

    let corrected = edit(
        store,
        &wrong.address(),
        FactPatch {
            content: Some("was never at the fair — a different weekend".into()),
            clear_edge: true,
            ..Default::default()
        },
    )
    .await;
    assert!(
        corrected.edge.is_none(),
        "the edge is still on a claim that now denies it: {corrected:?}",
    );
    // The rest of the patch still landed, so the empty cell is the argument
    // doing its work rather than the whole edit failing quietly.
    assert!(
        corrected.content.contains("never"),
        "the rewrite itself did not land: {corrected:?}",
    );
    assert_eq!(
        corrected.status,
        FactStatus::Active,
        "a disproved claim stays active — taking the edge off is not a retraction: \
         {corrected:?}",
    );
    assert_eq!(
        read_back(store, &never, &corrected.id).await.edge,
        None,
        "the empty cell did not survive the store",
    );

    // **The walk is the property.** The graph must stop answering through a
    // link the claim no longer draws, while the claim that still draws one
    // is still reached.
    let reached = graph::walk(
        store,
        &graph::GraphQuery {
            select: graph::Selection {
                subject: Some(fair.clone()),
                ..graph::Selection::default()
            },
            follow: Some(graph::Follow {
                along: graph::Along::Edge(EdgeShape::Attendance),
                direction: Some(graph::Direction::In),
                ..graph::Follow::hop()
            }),
            ..graph::GraphQuery::default()
        },
    )
    .await
    .expect("a walk from the fair")
    .objects;
    let guests: Vec<&EntityId> = reached[0].connected.iter().map(|o| &o.entity.id).collect();
    assert!(
        guests.contains(&&went),
        "the claim that still draws the edge stopped being reached: {guests:?}",
    );
    assert!(
        !guests.contains(&&never),
        "the walk still reaches somebody whose claim no longer points at the fair: \
         {guests:?}",
    );
}

/// 🚨 **A walk says when the claim behind a link was taken back — against
/// the store, through the retract verb.**
///
/// The resolver case proves the marker is computed. This proves it survives
/// the round trip: the status has to be written, stored and read back
/// before a walk can carry it, and a store that dropped it would answer the
/// resolver case identically.
///
/// **Marked, never filtered.** A link nobody stands behind and a link
/// nobody ever drew are different answers, and hiding the first would make
/// them one.
///
/// **Three reads, and each stops the others being vacuous:** the live link
/// unmarked says the marker is not on everything, the taken-back one marked
/// is the capability, and somebody no claim ever linked is reached by
/// neither.
pub async fn a_walk_marks_a_link_whose_claim_the_store_took_back<M: Memory>(store: &M) {
    let gathering = EntityId("event:contract-withdrawn-gathering".into());
    let stood = EntityId::person("person:contract-withdrawn-stood");
    let pulled = EntityId::person("person:contract-withdrawn-pulled");
    let apart = EntityId::person("person:contract-withdrawn-apart");
    ensure(store, &gathering).await;
    ensure(store, &apart).await;

    let attending = |who: &EntityId, said: &str| NewFact {
        edge: Some(Edge::new(EdgeShape::Attendance, gathering.clone())),
        ..NewFact::about(who.clone(), said, date(2026, 8, 10))
    };
    capture(store, attending(&stood, "was there")).await;
    let withdrawn = capture(store, attending(&pulled, "was there")).await;
    store
        .retract(
            &withdrawn.address(),
            Some("was never at it — a different evening"),
            date(2026, 8, 12),
        )
        .await
        .expect("a claim that stands may be taken back");

    let walked = graph::walk(
        store,
        &graph::GraphQuery {
            select: graph::Selection {
                subject: Some(gathering.clone()),
                ..graph::Selection::default()
            },
            follow: Some(graph::Follow {
                along: graph::Along::Edge(EdgeShape::Attendance),
                direction: Some(graph::Direction::In),
                ..graph::Follow::hop()
            }),
            ..graph::GraphQuery::default()
        },
    )
    .await
    .expect("a walk from the gathering")
    .objects;
    let reached = &walked[0].connected;
    let marker = |who: &EntityId| {
        reached
            .iter()
            .find(|o| &o.entity.id == who)
            .unwrap_or_else(|| panic!("{who:?} was not reached at all: {reached:?}"))
            .via
            .as_ref()
            .expect("a reached object says how the walk got to it")
            .retracted
    };

    assert!(
        !marker(&stood),
        "the claim that stands draws a link nothing marks: {reached:?}",
    );
    assert!(
        marker(&pulled),
        "the claim the store took back is still reached, and the link says so: {reached:?}",
    );
    assert!(
        !reached.iter().any(|o| o.entity.id == apart),
        "somebody no claim ever linked is reached by neither, so the pair above is about the \
         claims rather than about a walk that returns every person: {reached:?}",
    );
}

/// **A declared reference key is walkable against the store**, and the
/// declaration it rests on comes out of that same store.
///
/// The claim about storage is the one the pure resolver cannot make: the
/// walk asks the store what has been declared, so a store that keeps a
/// declaration but does not hand it back leaves every relation unfollowable
/// while every unit test over the resolver stays green.
///
/// The negative is the same walk with the type declared as TEXT rather than
/// a reference — same records, same values, and no link — so a build that
/// walked any key holding a handle fails here.
/// **A trip records who came, and the record answers from either end.**
///
/// The trip keys say where and when, and nothing on them says who — so a
/// companion is a link rather than a key. It is drawn as `attendance` from
/// the person at the trip, which is the shape that already means "was at":
/// a link drawn the other way could only be `about`, which asserts a claim
/// nobody made, or `connection`, which says the nature of the link was not
/// recorded.
///
/// **Both questions come out of that one edge**, and both are asserted
/// here, because the walk carries its own direction: *who came on this
/// trip* is the edge walked inbound, and *which trips was this person on*
/// is the same edge walked outbound from the person. The second is the one
/// a change would break silently, since nothing else in the suite asks it
/// of a trip.
///
/// **More than two companions on one trip**, which is what a key could not
/// hold: a thing's fields fold to the newest write of each key, so a key
/// would keep the last companion and drop the rest. Three would pass an
/// implementation that keeps only a pair.
/// **The kinds are rows, and a shipped one is closed to a caller.**
///
/// Only a store can answer this: the set a process parses against is
/// loaded from these rows, so a store that cannot keep them is a store
/// where no handle resolves. The double keeps whatever it is handed, which
/// is why this belongs to the contract rather than to a unit test.
pub async fn the_kinds_are_rows_and_a_shipped_one_is_closed<M: Memory>(store: &M) {
    // The two steps a boot takes, in the order it takes them: write what
    // this build ships, then parse against what the store answers with.
    crate::memory::kinds::seed(store)
        .await
        .expect("the kinds are seeded");

    let held = store.declared_kinds().await.expect("the kinds read back");
    for shipped in crate::memory::kinds::SHIPPED {
        assert!(
            held.iter()
                .any(|(token, origin)| token == shipped && *origin == Origin::Shipped),
            "the store holds '{shipped}' as a kind the software ships: {held:?}",
        );
    }

    // **Re-seeding changes nothing**, which is what lets the boot write
    // them unconditionally.
    store
        .declare_kind("person", Origin::Shipped, Vec::new())
        .await
        .expect("the seed runs again");
    let after = store.declared_kinds().await.expect("the kinds read back");
    assert_eq!(
        held.len(),
        after.len(),
        "a second seed writes no second row: {after:?}",
    );

    // **A caller cannot take one over.** The refusal reads the row's own
    // origin, so nothing anywhere keeps a list of protected names.
    let refused = store
        .declare_kind("person", Origin::Declared, Vec::new())
        .await
        .expect_err("a caller cannot redeclare a kind the software ships");
    assert!(
        matches!(refused, MemoryError::InvalidEntity(_)),
        "the refusal says what is wrong rather than failing the store: {refused:?}",
    );
    // **The caller's own layer, and the same write lands in it.** A kind
    // of their own is theirs to declare, which is what the refusal above
    // is a refusal to do somewhere else rather than a refusal to do at
    // all.
    store
        .declare_kind("theta", Origin::Declared, Vec::new())
        .await
        .expect("a caller declares a kind of their own");

    // **Both answers out of ONE read.** The refusal alone passes on a
    // store that refuses every declaration there is; the caller's own row
    // alone passes on a store that refuses nothing.
    let held = store.declared_kinds().await.expect("the kinds read back");
    assert_eq!(
        held.iter()
            .find(|(token, _)| token == "person")
            .map(|(_, origin)| *origin),
        Some(Origin::Shipped),
        "the row it refused to take over is untouched: {held:?}",
    );
    assert_eq!(
        held.iter()
            .find(|(token, _)| token == "theta")
            .map(|(_, origin)| *origin),
        Some(Origin::Declared),
        "…and the same write under a name of the caller's own landed: {held:?}",
    );
}

/// **A row the binary owns is reconciled on every boot, never upserted.**
///
/// The seed writes what this build ships. What it could not do was take
/// back what an OLDER build shipped: a kind dropped from the set sat on
/// every upgraded instance for ever, and nothing on the row said it was
/// the software's rather than the operator's.
///
/// **Three answers out of ONE read**, because each covers how the other
/// two pass on a build that is wrong. The reclaimed row alone passes on a
/// build that empties the table. The surviving caller's row alone passes
/// on a build that removes nothing. The shipped set alone passes on a
/// build that has never heard of an older one.
///
/// Only a store can answer this: the set a process parses against is
/// loaded from these rows, and the double keeps whatever it is handed.
pub async fn an_owned_kind_this_build_dropped_is_reclaimed<M: Memory>(store: &M) {
    // **What an older build shipped and this one does not.** The origin is
    // the whole marker: nothing else on the row says who wrote it.
    store
        .declare_kind("zeta", Origin::Shipped, Vec::new())
        .await
        .expect("an older build declares its own kinds");
    // **A caller's row of the same shape**, which reconciling must not
    // touch. Same table, same columns, one column different.
    store
        .declare_kind("eta", Origin::Declared, Vec::new())
        .await
        .expect("a caller declares a kind of their own");

    crate::memory::kinds::seed(store)
        .await
        .expect("the kinds are reconciled");

    let held = store.declared_kinds().await.expect("the kinds read back");
    assert!(
        !held.iter().any(|(token, _)| token == "zeta"),
        "the build stopped shipping 'zeta', so the instance stops holding it: {held:?}",
    );
    assert_eq!(
        held.iter()
            .find(|(token, _)| token == "eta")
            .map(|(_, origin)| *origin),
        Some(Origin::Declared),
        "…and the row the caller wrote is exactly where they left it: {held:?}",
    );
    for shipped in crate::memory::kinds::SHIPPED {
        assert!(
            held.iter()
                .any(|(token, origin)| token == shipped && *origin == Origin::Shipped),
            "…and this build's own set is whole, so reconciling removed \
             what left the set and nothing else: {held:?}",
        );
    }
}

pub async fn a_trip_records_who_came_and_answers_from_either_end<M: Memory>(store: &M) {
    let away = EntityId("event:contract-long-weekend".into());
    let home = EntityId("place:contract-harbour-end".into());
    let there = EntityId("place:contract-fjord-town".into());
    for id in [&away, &home, &there] {
        ensure(store, id).await;
    }
    let companions = [
        EntityId::person("person:contract-omicron"),
        EntityId::person("person:contract-sigma"),
        EntityId::person("person:contract-tau"),
        EntityId::person("person:contract-upsilon"),
    ];

    // The trip itself: the keys the shipped type names, so this is a trip
    // rather than any event that happens to have guests.
    capture(
        store,
        NewFact {
            fields: [
                ("departs_from".to_string(), home.to_string()),
                ("arrives_at".to_string(), there.to_string()),
                ("leaves_on".to_string(), "2026-05-01".to_string()),
                ("returns_on".to_string(), "2026-05-04".to_string()),
            ]
            .into_iter()
            .collect(),
            refs: Vec::new(),
            ..NewFact::about(away.clone(), "four days away", date(2026, 4, 1))
        },
    )
    .await;

    for who in &companions {
        ensure(store, who).await;
        capture(
            store,
            NewFact {
                edge: Some(Edge::new(EdgeShape::Attendance, away.clone())),
                ..NewFact::about(who.clone(), "came along", date(2026, 5, 1))
            },
        )
        .await;
    }

    // **Who came on this trip** — inbound, and the companions are reached
    // rather than named.
    let came = walked_from(store, &away, graph::Direction::In).await;
    for who in &companions {
        assert!(
            came.contains(who),
            "every companion hangs off the trip, and {who} does not: {came:?}",
        );
    }

    // **Which trips was this person on** — the same edge, outbound from the
    // person's end, which is the half nothing else asks.
    let went = walked_from(store, &companions[0], graph::Direction::Out).await;
    assert!(
        went.contains(&away),
        "the trip is reached from the companion it carried: {went:?}",
    );

    // **A trip nobody came on still reads back**, so recording companions
    // is something a trip may do rather than something it must.
    let alone = EntityId("event:contract-lone-crossing".into());
    ensure(store, &alone).await;
    capture(
        store,
        NewFact {
            fields: [
                ("departs_from".to_string(), home.to_string()),
                ("arrives_at".to_string(), there.to_string()),
            ]
            .into_iter()
            .collect(),
            refs: Vec::new(),
            ..NewFact::about(alone.clone(), "went by myself", date(2026, 6, 1))
        },
    )
    .await;
    let read = graph::walk(
        store,
        &graph::GraphQuery {
            select: graph::Selection {
                subject: Some(alone.clone()),
                ..graph::Selection::default()
            },
            ..graph::GraphQuery::default()
        },
    )
    .await
    .expect("a handle is a selection")
    .objects;
    assert_eq!(
        read.first().map(|o| &o.entity.id),
        Some(&alone),
        "a trip with no companions comes back as itself: {read:?}",
    );
    assert!(
        walked_from(store, &alone, graph::Direction::In)
            .await
            .is_empty(),
        "…and nobody is reached from it",
    );
}

/// The entities one attendance hop reaches from this one, in the direction
/// asked. The two directions are two questions, which is why the direction
/// is the argument.
async fn walked_from<M: Memory>(
    store: &M,
    from: &EntityId,
    direction: graph::Direction,
) -> Vec<EntityId> {
    let walked = graph::walk(
        store,
        &graph::GraphQuery {
            select: graph::Selection {
                subject: Some(from.clone()),
                ..graph::Selection::default()
            },
            include: graph::Include {
                facts: false,
                prose: false,
                stood_for: false,
            },
            follow: Some(graph::Follow {
                along: graph::Along::Edge(EdgeShape::Attendance),
                direction: Some(direction),
                depth: 1,
                keeping: Vec::new(),
                fits_type: None,
            }),
            history: None,
        },
    )
    .await
    .expect("a subject with a walk")
    .objects;
    walked
        .first()
        .map(|o| o.connected.iter().map(|c| c.entity.id.clone()).collect())
        .unwrap_or_default()
}

pub async fn a_declared_reference_key_is_walkable_against_the_store<M: Memory>(store: &M) {
    let owner = EntityId::person("person:contract-relation-owner");
    let held = EntityId("thing:contract-relation-held".into());
    ensure(store, &owner).await;
    ensure(store, &held).await;

    capture(
        store,
        NewFact {
            fields: [
                ("keeper".to_string(), owner.to_string()),
                ("since".to_string(), "2019-04-15".to_string()),
            ]
            .into_iter()
            .collect(),
            refs: Vec::new(),
            ..NewFact::about(held.clone(), "the one that is held", date(2026, 8, 10))
        },
    )
    .await;

    let declare = |holds: ValueType| async move {
        store
            .declare_type(DeclaredType::new(
                "contract-holding",
                vec![
                    Field::required("keeper", holds),
                    Field::required("since", ValueType::Date),
                ],
            ))
            .await
            .expect("a declaration the store keeps");
    };
    let reached = |relation: &str| {
        let relation = relation.to_string();
        let subject = owner.clone();
        async move {
            graph::walk(
                store,
                &graph::GraphQuery {
                    select: graph::Selection {
                        subject: Some(subject),
                        ..graph::Selection::default()
                    },
                    include: graph::Include {
                        facts: false,
                        prose: false,
                        stood_for: false,
                    },
                    follow: Some(graph::Follow {
                        along: graph::Along::Relation(relation),
                        direction: Some(graph::Direction::In),
                        ..graph::Follow::hop()
                    }),
                    history: None,
                },
            )
            .await
        }
    };

    declare(ValueType::Text).await;
    reached("keeper")
        .await
        .expect_err("a key declared to hold text is no relation, whatever its value looks like");

    declare(ValueType::Reference).await;
    let found = reached("keeper")
        .await
        .expect("declared a reference, the key is a relation")
        .objects;
    let connected: Vec<&EntityId> = found[0].connected.iter().map(|o| &o.entity.id).collect();
    assert!(
        connected.contains(&&held),
        "the key walked inbound reaches what points at this object: {connected:?}",
    );

    // And the ordering the same declaration licenses, over a value that
    // survived storage rather than one this test still holds.
    let older = graph::walk(
        store,
        &graph::GraphQuery {
            select: graph::Selection {
                fields: vec![graph::FieldFilter::comparing(
                    "since",
                    crate::memory::types::Compare::Before,
                    "2020-01-01",
                )],
                ..graph::Selection::default()
            },
            ..graph::GraphQuery::default()
        },
    )
    .await
    .expect("a declared date licenses an ordering")
    .objects;
    assert!(
        older.iter().any(|o| o.entity.id == held),
        "the record stored before that date is selected by the ordering: {older:?}",
    );
}

/// **The kind a reference points at survives the store.**
///
/// A store keeps a declaration as one token, so a narrowing it can write
/// and not read back is a declaration that silently widens on the next
/// read — and every check that rests on it then passes a handle of any
/// kind. Only a store can answer for that, which is why this is a contract
/// case.
///
/// The unnarrowed reference is in the same type, because a store that read
/// every reference back as one kind would satisfy the first assertion
/// alone.
///
/// **One of the narrowings names the LONGEST kind, and that is not
/// decoration.** A declaration crosses to a store as one token —
/// `reference:` and the kind — so the kinds do not all cost the same
/// number of characters, and a cell wide enough for most of them holds a
/// declaration that reads back as any handle at all, or fails on the
/// write. A case that picks a short kind answers for the short kinds only.
pub async fn a_reference_keeps_the_kind_it_points_at<M: Memory>(store: &M) {
    store
        .declare_type(DeclaredType::new(
            "contract-stay",
            vec![
                Field::pointing_at("venue", EntityKind::PLACE).needed(),
                Field::pointing_at("part_of", EntityKind::PROJECT).needed(),
                Field::required("booked_by", ValueType::Reference),
            ],
        ))
        .await
        .expect("declaring a type of my own is accepted");

    let held = store
        .declared_types()
        .await
        .expect("the roster reads")
        .into_iter()
        .find(|t| t.name == "contract-stay")
        .expect("the store holds the type it just took");
    assert_eq!(
        held.field("venue").and_then(|f| f.points_at),
        Some(EntityKind::PLACE),
        "the narrowing came back off the store: {held:?}",
    );
    assert_eq!(
        held.field("part_of").and_then(|f| f.points_at),
        Some(EntityKind::PROJECT),
        "…and so did the one naming the longest kind, which is the token a cell sized for \
         the others cannot hold: {held:?}",
    );
    assert_eq!(
        held.field("booked_by").and_then(|f| f.points_at),
        None,
        "…and a reference that named no kind is still any handle: {held:?}",
    );
}

/// **Neither half may write over the other's keys.**
///
/// A kind's keys and a type's keys live in one place, so the name alone
/// cannot say which of the two wrote a row. Sharing the place is the model
/// working — a kind IS a schema — and taking the other side's keys is not:
/// a declaration that silently replaced them would move a kind's floor
/// from a verb nobody called.
///
/// **Both directions, because the two are separate checks and either can
/// go missing on its own.**
///
/// **The narrow reach is the point rather than a limitation.** It needs a
/// kind whose keys a CALLER declared: on a kind the software ships, the
/// origin check refuses the caller first, in both stores. So this is the
/// one door where the two halves can reach each other at all.
///
/// A contract case because a store answering this differently from the
/// double is what a case belonging to either alone cannot see.
pub async fn neither_half_writes_over_the_others_keys<M: Memory>(store: &M) {
    // ① A type may not take a kind's keys over.
    store
        .declare_kind(
            "handcart",
            Origin::Declared,
            vec![Field::required("load", ValueType::Number)],
        )
        .await
        .expect("a caller declares a kind that names a key");
    let refused = store
        .declare_type(DeclaredType::new(
            "handcart",
            vec![Field::required("colour", ValueType::Text)],
        ))
        .await
        .expect_err("a type may not write over the keys of a kind of that name");
    assert!(
        matches!(refused, MemoryError::InvalidEntity(_)),
        "the declaration is what is wrong, not the store: {refused:?}",
    );

    // **…and the kind's keys are where they were.** Without this the case
    // passes against a store that refuses the declaration and replaces the
    // keys anyway.
    let held = store
        .declared_types()
        .await
        .expect("the roster reads")
        .into_iter()
        .find(|t| t.name == "handcart")
        .expect("the kind's keys are still held under its name");
    assert!(
        held.field("load").is_some() && held.field("colour").is_none(),
        "the kind kept its own keys and took none of the type's: {held:?}",
    );

    // ② And a kind may not take a type's keys over, which is the same rule
    // read from the other side.
    store
        .declare_type(DeclaredType::new(
            "contract-cartload",
            vec![Field::required("weighed_on", ValueType::Date)],
        ))
        .await
        .expect("a caller declares a type of their own");
    let refused = store
        .declare_kind(
            "contract-cartload",
            Origin::Declared,
            vec![Field::required("load", ValueType::Number)],
        )
        .await
        .expect_err("a kind may not write over the keys of a type of that name");
    assert!(
        matches!(refused, MemoryError::InvalidEntity(_)),
        "the declaration is what is wrong, not the store: {refused:?}",
    );
    let held = store
        .declared_types()
        .await
        .expect("the roster reads")
        .into_iter()
        .find(|t| t.name == "contract-cartload")
        .expect("the type's keys are still held under its name");
    assert!(
        held.field("weighed_on").is_some() && held.field("load").is_none(),
        "the type kept its own keys and took none of the kind's: {held:?}",
    );
}

/// **A caller's type named after a kind governs nothing, and the kind's
/// own keys still govern everything.**
///
/// A kind's keys and a type's keys live in one place and are told apart by
/// who declared them. The fit guard selects a declaration whose NAME
/// matches the thing's kind, so a guard handed both halves as one list
/// reads a caller's type named `place` as the kind `place`'s schema — a
/// gate over a shipped kind, made with no new verb and with no verb to
/// undo it.
///
/// **The declaration itself stays legal.** A caller's schema may be named
/// after a kind, and a restart must not destroy it. What is wrong is the
/// lookup, so the halves are kept apart and the kind's keys reach the
/// guard as the kind's.
///
/// **A contract case because the two stores answered this differently.**
/// One read the halves apart and the other did not, and a case belonging
/// to either alone is a case that cannot see the difference. The tooth is
/// the value-type one on an ordinary write with no floor involved, which
/// is the widest blast radius and the one a fix aimed at fitting walks
/// past.
pub async fn a_type_named_after_a_kind_does_not_gate_that_kinds_writes<M: Memory>(store: &M) {
    // A shipped kind that names no key, so what governs a place here can
    // only be the caller's type below.
    let moes = EntityId("place:contract-tavern".into());
    ensure(store, &moes).await;
    store
        .declare_type(DeclaredType::new(
            "place",
            vec![Field::required("shadow_postcode", ValueType::Number)],
        ))
        .await
        .expect("a caller's schema may be named after a kind");

    // **The write the shadow refused.** A place carrying a postcode that is
    // no number is an ordinary claim: nothing a caller declared governs it.
    capture(
        store,
        NewFact {
            fields: [("shadow_postcode".to_string(), "SW1A".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(moes.clone(), "the postcode is SW1A", date(2026, 8, 19))
        },
    )
    .await;

    // **The positive the negative rests on: a kind's OWN key still holds a
    // write to what it says.** Without this half the case passes against a
    // build whose fit guard reads nothing at all and lets everything
    // through.
    //
    // A shipped kind is used rather than a new one, for the reason the
    // cases above give: declaring a new kind fills the set this process
    // parses against.
    store
        .declare_kind(
            "pet",
            Origin::Shipped,
            vec![Field::required("pet_weight", ValueType::Number)],
        )
        .await
        .expect("a kind may name the keys its things keep");
    let helper = EntityId("pet:contract-the-heavy-one".into());
    ensure(store, &helper).await;
    let refused = store
        .capture(NewFact {
            fields: [("pet_weight".to_string(), "quite a lot".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(helper, "it weighs a fair bit", date(2026, 8, 19))
        })
        .await;
    let Err(MemoryError::BreaksType { name, key, .. }) = &refused else {
        panic!("a kind's own key must hold a write to what it says, got {refused:?}");
    };
    assert_eq!(name, "pet", "the refusal names the kind");
    assert_eq!(key, "pet_weight", "…and the key it wrote badly");
}

/// **A write cannot put a value on a thing that its type refuses — and
/// the same write against a thing that fits nothing is allowed.**
///
/// Holding a key badly is not holding it, so a value that breaks its key's
/// declaration drops the thing below the type exactly as taking the key
/// away would. The refusal is the floor rule reaching a second way of
/// losing a key, never a second rule.
///
/// Four beats, and the last three are what stop this becoming a gate: the
/// wrong-kinded handle is refused on a thing that IS a stay; the identical
/// write lands on a thing that fits nothing, which is what keeps a
/// half-described thing repairable; a key no type mentions is welcome on
/// the fitting thing, because strict is a floor; and the refused record is
/// unchanged afterwards.
///
/// A contract case because only a store can say whether the write was
/// refused AND the record kept.
pub async fn a_write_cannot_put_a_value_the_type_refuses<M: Memory>(store: &M) {
    // **A KIND, not a declared type.** What a write may take off a thing is
    // its kind's question: a declared type describes and holds nothing.
    store
        .declare_kind(
            "work",
            Origin::Shipped,
            vec![
                Field::pointing_at("stay_venue", EntityKind::PLACE).needed(),
                Field::required("stay_nights", ValueType::Number),
            ],
        )
        .await
        .expect("a kind may name the keys its things keep");
    // a shipped kind is used rather than a new one:
    // declaring a new kind fills the set this process parses against, and any
    // case beside this one that stands a store up empties it again.

    let moes = EntityId("place:contract-moes".into());
    let helper = EntityId("pet:contract-santas-little-helper".into());
    ensure(store, &moes).await;
    ensure(store, &helper).await;

    // A thing that fits: the venue is a place and the nights are a number.
    let stay = EntityId("work:contract-the-stay".into());
    let booked = capture(
        store,
        NewFact {
            fields: [
                ("stay_venue".to_string(), moes.to_string()),
                ("stay_nights".to_string(), "2".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(stay.clone(), "two nights booked", date(2026, 4, 18))
        },
    )
    .await;

    // ① The wrong kind on a thing that fits is refused, and the refusal
    // says which key and what it wanted.
    let refused = store
        .update_fact(
            &booked.address(),
            FactPatch {
                fields: [("stay_venue".to_string(), helper.to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
        )
        .await;
    let Err(MemoryError::BreaksType {
        name,
        key,
        wanted,
        value,
    }) = &refused
    else {
        panic!("a handle of the wrong kind must be refused, got {refused:?}");
    };
    assert_eq!(name, "work", "the refusal names the kind");
    assert_eq!(key, "stay_venue", "…and the key");
    assert_eq!(
        wanted, "reference:place",
        "…and what the key wanted, which `reference` alone could not say",
    );
    assert_eq!(value, &helper.to_string(), "…and what was actually sent");

    // ② The record is exactly as it was: a refusal writes nothing.
    assert_eq!(
        read_back(store, &stay, &booked.id)
            .await
            .fields
            .get("stay_venue")
            .map(String::as_str),
        Some(moes.to_string().as_str()),
        "the refused write left the record alone",
    );

    // ③ **The same write, on a thing that fits nothing.** It carries one of
    // the type's keys and not the other, so there is no fit to protect and
    // the store takes the value as written. Without this beat the case
    // above passes on a build that refuses every reference everywhere.
    let sketch = EntityId("event:contract-the-sketch".into());
    let jotted = capture(
        store,
        NewFact {
            fields: [("stay_venue".to_string(), moes.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(sketch.clone(), "somewhere, some time", date(2026, 4, 18))
        },
    )
    .await;
    store
        .update_fact(
            &jotted.address(),
            FactPatch {
                fields: [("stay_venue".to_string(), helper.to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
        )
        .await
        .expect("a thing that fits nothing has nothing to protect")
        .written()
        .expect("…and the write lands");

    // ④ **Adding a key no type mentions is never refused**, on the thing
    // that does fit. Strict is a floor: a type says what must survive, not
    // what may be there.
    store
        .update_fact(
            &booked.address(),
            FactPatch {
                fields: [("stay_mood".to_string(), "quiet".to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
        )
        .await
        .expect("a key beyond the type is welcome")
        .written()
        .expect("…and it lands");

    // ⑤ **The other door.** A thing's fields are every write on it folded,
    // so a NEW record carrying the same bad value takes the key on the
    // thing just as an edit does. Guarding only the edit would leave the
    // rule true of one verb and the thing broken by the other.
    let captured = store
        .capture(NewFact {
            fields: [("stay_venue".to_string(), helper.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(stay.clone(), "moved it, so they say", date(2026, 4, 19))
        })
        .await;
    assert!(
        matches!(&captured, Err(MemoryError::BreaksType { key, .. }) if key == "stay_venue"),
        "a capture carrying the wrong kind is refused too, got {captured:?}",
    );
    // …and the same capture against the thing that fits nothing lands, so
    // this cannot pass on a build where capture refuses every reference.
    store
        .capture(NewFact {
            fields: [("stay_venue".to_string(), helper.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(sketch.clone(), "or maybe there", date(2026, 4, 19))
        })
        .await
        .expect("a thing that fits nothing has nothing to protect")
        .written()
        .expect("…and the capture lands");
}

/// **A reference naming an entity nobody recorded is refused, the way a
/// missing edge object already is.**
///
/// A reference is a walkable link, so a value naming nothing is a link into
/// a node no question can reach — the same hole an edge into a missing
/// entity leaves, arrived at through a key instead of an edge. It comes
/// back blocked with candidates rather than as an error, because the repair
/// is the caller's and the near handles are what it needs.
///
/// **Only a value that is a HANDLE is asked about.** A reference key
/// holding a phrase is a value that fails its declaration, which is the
/// floor's business and not this one's — asking whether "the airport"
/// exists would refuse loose prose in the name of a link nobody drew.
///
/// The real target is written in the same shape, so this cannot pass on a
/// build that refuses every reference.
/// **A key declared to hold one of a named set refuses a value outside it,
/// and the set survives storage to say so.**
///
/// A contract case rather than a unit one, and the storage is the reason:
/// the values are part of the declaration, so a store that keeps the key
/// and drops its set narrows nothing. Every beat below passes on such a
/// store except the refusal itself.
pub async fn a_closed_set_refuses_a_write_outside_it<M: Memory>(store: &M) {
    // A shipped KIND, because what a write may put on a thing is its kind's
    // question. A declared type describes and governs nothing.
    store
        .declare_kind(
            "project",
            Origin::Shipped,
            vec![Field::one_of("project_stage", ["draft", "building", "done"]).needed()],
        )
        .await
        .expect("a kind may narrow a key to a named set");

    // ⓪ **The set came back off the store.** Read before anything is
    // written, so a failure here is storage rather than the guard.
    let held = store
        .declared_types()
        .await
        .expect("the declarations read back");
    let stage = held
        .iter()
        .find(|d| d.name == "project")
        .and_then(|d| d.field("project_stage"))
        .expect("the kind holds the key it declared");
    assert_eq!(
        stage.one_of.as_deref(),
        Some(["draft", "building", "done"].map(String::from).as_slice()),
        "the values are part of the declaration, so they survive storage",
    );

    // ① A member of the set lands.
    let sketch = EntityId("project:contract-the-sketchbook".into());
    ensure(store, &sketch).await;
    let opened = capture(
        store,
        NewFact {
            fields: [("project_stage".to_string(), "draft".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(sketch.clone(), "started it", date(2026, 4, 18))
        },
    )
    .await;

    // ② A value outside the set is refused, and the refusal NAMES the
    // values — `text` is what the key holds underneath and would tell a
    // caller nothing about a value that is text.
    let refused = store
        .update_fact(
            &opened.address(),
            FactPatch {
                fields: [("project_stage".to_string(), "shipped".to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
        )
        .await;
    let Err(MemoryError::BreaksType {
        name,
        key,
        wanted,
        value,
    }) = &refused
    else {
        panic!("a value outside the set must be refused, got {refused:?}");
    };
    assert_eq!(name, "project", "the refusal names the kind");
    assert_eq!(key, "project_stage", "…and the key");
    for allowed in ["draft", "building", "done"] {
        assert!(
            wanted.contains(allowed),
            "…and every value the caller may write: {wanted:?}",
        );
    }
    assert_eq!(value, "shipped", "…and what was actually sent");

    // ③ The record is exactly as it was: a refusal writes nothing.
    assert_eq!(
        read_back(store, &sketch, &opened.id)
            .await
            .fields
            .get("project_stage")
            .map(String::as_str),
        Some("draft"),
        "the refused write left the record alone",
    );

    // ④ **Another member lands on the same thing.** Without this the
    // refusal above passes on a build that refuses every write to the key.
    store
        .update_fact(
            &opened.address(),
            FactPatch {
                fields: [("project_stage".to_string(), "building".to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
        )
        .await
        .expect("a value the set names is written")
        .written()
        .expect("…and the write lands");

    // ⑤ **A thing below the floor is refused NOTHING.** It carries no key
    // of this kind, so there is no fit to protect, and the same value the
    // fitting thing was refused is taken as written. That is what keeps a
    // messy record repairable.
    let jotted = EntityId("event:contract-the-jotting".into());
    ensure(store, &jotted).await;
    store
        .capture(NewFact {
            fields: [("project_stage".to_string(), "shipped".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(jotted.clone(), "somewhere in it", date(2026, 4, 18))
        })
        .await
        .expect("a thing with no fit to protect takes the value as written")
        .written()
        .expect("…and the write lands");
}

pub async fn a_reference_must_name_an_entity_that_exists<M: Memory>(store: &M) {
    store
        .declare_type(DeclaredType::new(
            "contract-loan",
            vec![Field::required("loaned_to", ValueType::Reference)],
        ))
        .await
        .expect("declaring a type of my own is accepted");

    let borrower = EntityId("person:contract-milhouse".into());
    ensure(store, &borrower).await;
    let ledger = EntityId("thing:contract-the-ledger".into());
    ensure(store, &ledger).await;

    let missing = store
        .capture(NewFact {
            fields: [(
                "loaned_to".to_string(),
                "person:contract-nobody".to_string(),
            )]
            .into_iter()
            .collect(),
            ..NewFact::about(ledger.clone(), "lent it to somebody", date(2026, 5, 1))
        })
        .await
        .expect("a miss is an answer, not a failure");
    let Guarded::Blocked { attempted, .. } = &missing else {
        panic!("a reference to nobody must be blocked, got {missing:?}");
    };
    assert_eq!(
        attempted.as_str(),
        "person:contract-nobody",
        "the answer names the handle that missed",
    );

    // …and the same key naming somebody who exists goes straight through.
    store
        .capture(NewFact {
            fields: [("loaned_to".to_string(), borrower.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(ledger.clone(), "lent it to Milhouse", date(2026, 5, 2))
        })
        .await
        .expect("capture succeeds")
        .written()
        .expect("a reference to somebody real lands");

    // **The edit path names entities too.** A patch setting a reference key
    // is a write that names one, so it faces the same rule — otherwise the
    // link nobody could capture could still be edited into place.
    let lent = capture(
        store,
        NewFact {
            fields: [("loaned_to".to_string(), borrower.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(ledger.clone(), "out on loan", date(2026, 5, 4))
        },
    )
    .await;
    let edited = store
        .update_fact(
            &lent.address(),
            FactPatch {
                fields: [(
                    "loaned_to".to_string(),
                    "person:contract-nobody".to_string(),
                )]
                .into_iter()
                .collect(),
                ..Default::default()
            },
        )
        .await
        .expect("a miss is an answer, not a failure");
    assert!(
        matches!(&edited, Guarded::Blocked { attempted, .. }
                 if attempted.as_str() == "person:contract-nobody"),
        "an edit to a reference nobody recorded is blocked too, got {edited:?}",
    );

    // **A value that is no handle is not asked to exist.** It fails its
    // declaration and that is the floor's question; this thing fits no
    // type, so nothing refuses it and the note stays writable.
    let scrap = EntityId("thing:contract-the-scrap".into());
    ensure(store, &scrap).await;
    store
        .capture(NewFact {
            fields: [("loaned_to".to_string(), "somebody at the shop".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(scrap, "lent it to somebody, no idea who", date(2026, 5, 3))
        })
        .await
        .expect("capture succeeds")
        .written()
        .expect("a phrase under a reference key is messy, not a broken link");
}

/// **A kind the code learned today survives the store.**
///
/// The store keeps a kind as a string and reads it back off the handle, so
/// a kind arriving in the enum needs nothing migrated — but that is a claim
/// **A write cannot drop a thing below a type it already fits — and the
/// same write against a thing that fits nothing is allowed.**
///
/// Three claims that only hold together. The refusal protects a thing that
/// IS something; the acceptance keeps a thing that is not something
/// repairable, which is the whole reason the check reads the result rather
/// than the change; and adding keys is never refused, because strict here
/// is a floor and not a ceiling.
///
/// A contract case rather than a domain one: what is under test is that a
/// STORE refuses the write and keeps the record, which only a store can
/// answer for.
pub async fn a_write_cannot_break_a_fit_that_already_exists<M: Memory>(store: &M) {
    // **A KIND, not a declared type.** What a write may take off a thing is
    // its kind's question: a thing is held to what its own kind names, and to nothing else.
    store
        .declare_kind(
            "org",
            Origin::Shipped,
            vec![
                Field::required("cost", ValueType::Number),
                Field::required("done_on", ValueType::Date),
                // **Optional, and never written on the thing below.** The
                // floor is the REQUIRED keys, so a thing holding those two
                // is at it whether or not this one is there. A guard
                // measured against every key a kind names would decide that
                // thing fits nothing and stop protecting it — which is the
                // failure the refusal below catches.
                Field::new("note", ValueType::Text),
            ],
        )
        .await
        .expect("a kind may name the keys its things keep");
    // a shipped kind is used rather than a new one, and `thing`
    // is left keyless for the case below that asks the other half.

    // A thing that fits: both REQUIRED keys, over two sittings, because
    // that is how things get written down. **It never holds the optional
    // one**, which is what makes the refusal below answer for the floor: a
    // guard measured against every key a kind names would decide this thing
    // fits nothing and let the required key go.
    let whole = EntityId("org:contract-fitting-thing".into());
    let costed = capture(
        store,
        NewFact {
            fields: [("cost".to_string(), "40".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(whole.clone(), "the annual service", date(2026, 4, 18))
        },
    )
    .await;
    capture(
        store,
        NewFact {
            fields: [("done_on".to_string(), "2026-04-18".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(whole.clone(), "and it was done that day", date(2026, 4, 18))
        },
    )
    .await;

    let refused = store
        .update_fact(
            &costed.address(),
            FactPatch {
                clear_fields: vec!["cost".to_string()],
                ..Default::default()
            },
        )
        .await;
    let Err(MemoryError::BreaksFit { name, keys }) = &refused else {
        panic!("taking a key off a thing that fits must be refused, got {refused:?}");
    };
    assert_eq!(name, "org", "the refusal names the kind");
    assert!(
        keys.contains(&"cost".to_string()),
        "…and the key that would go: {keys:?}"
    );
    assert_eq!(
        read_back(store, &whole, &costed.id)
            .await
            .fields
            .get("cost")
            .map(String::as_str),
        Some("40"),
        "a refused write leaves the record exactly as it was"
    );

    // **The same write, against a thing that fits nothing: allowed.** A
    // thing with one of the two keys was never a service, so there is
    // nothing here to protect — and a rule that read the CHANGE rather
    // than the result would refuse this one too and leave the record
    // unrepairable.
    let partial = EntityId("org:contract-loose-record".into());
    let half = capture(
        store,
        NewFact {
            fields: [("cost".to_string(), "40".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                partial.clone(),
                "somebody wrote down a price",
                date(2026, 4, 18),
            )
        },
    )
    .await;
    edit(
        store,
        &half.address(),
        FactPatch {
            clear_fields: vec!["cost".to_string()],
            ..Default::default()
        },
    )
    .await;
    assert!(
        !read_back(store, &partial, &half.id)
            .await
            .fields
            .contains_key("cost"),
        "a thing that fits nothing has nothing to protect, so the key goes"
    );

    // **Adding is never refused**, including a key no type names: the
    // floor is what a type asks for, and everything above it is welcome.
    let added = edit(
        store,
        &costed.address(),
        FactPatch {
            fields: [
                ("cost".to_string(), "45".to_string()),
                ("mechanic".to_string(), "somebody at the yard".to_string()),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        added.fields.get("mechanic").map(String::as_str),
        Some("somebody at the yard"),
        "a key no type mentions is welcome on a thing that fits one"
    );
    assert_eq!(added.fields.get("cost").map(String::as_str), Some("45"));
}

/// **Moving a record past is refused when it would break a fit; taking it
/// back is not.**
///
/// A supersede and a clear are the same act with two spellings — both take
/// a key out of what the thing carries, neither is ceremonious, and a rule
/// that refused one and served the other would read as arbitrary.
///
/// **Retraction is the other kind of write and is left alone.** It is
/// one-way, it is deliberate, and it says a record should never have been
/// written; refusing it because of what it costs a type would make a claim
/// somebody wants taken back impossible to take back. Guard the ordinary
/// writes, never the deliberate ones.
///
/// Both in one case, on one record, because each half passes on its own
/// against a build that refuses everything or nothing.
pub async fn a_supersede_that_breaks_a_fit_is_refused_and_a_retraction_is_not<M: Memory>(
    store: &M,
) {
    // **A KIND, not a declared type.** A supersede costs the thing a key,
    // and what a thing may not lose is what its own kind names.
    store
        .declare_kind(
            "topic",
            Origin::Shipped,
            vec![
                Field::required("season", ValueType::Text),
                Field::required("pitch_fee", ValueType::Number),
            ],
        )
        .await
        .expect("a kind may name the keys its things keep");
    // a shipped kind is used rather than a new one, for the reason
    // the case above gives.

    let held = EntityId("topic:contract-run-of-stalls".into());
    let seasonal = capture(
        store,
        NewFact {
            fields: [("season".to_string(), "summer".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(held.clone(), "the season it runs in", date(2026, 4, 18))
        },
    )
    .await;
    capture(
        store,
        NewFact {
            fields: [("pitch_fee".to_string(), "14".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(held.clone(), "and what a pitch costs", date(2026, 4, 18))
        },
    )
    .await;

    // Only active records fold, so moving this one past takes its key out
    // of what the thing carries — the same cost a clear has.
    let refused = store
        .update_fact(
            &seasonal.address(),
            FactPatch {
                status: Some(FactStatus::Archived),
                ..Default::default()
            },
        )
        .await;
    let Err(MemoryError::BreaksFit { name, keys }) = &refused else {
        panic!("moving a record past must cost what a clear costs, got {refused:?}");
    };
    assert_eq!(name, "topic");
    assert!(keys.contains(&"season".to_string()), "{keys:?}");
    assert_eq!(
        read_back(store, &held, &seasonal.id).await.status,
        FactStatus::Active,
        "a refused write leaves the record where it was"
    );

    // **The same record, taken back: served.** Retraction is deliberate and
    // one-way, and a claim somebody wants taken back has to be takeable
    // back whatever it costs a type.
    let taken = store
        .retract(
            &seasonal.address(),
            Some("it was never so"),
            date(2026, 4, 19),
        )
        .await
        .expect("a retraction is not refused by the fit guard");
    assert_eq!(taken.retracted.status, FactStatus::Archived);
    assert_eq!(
        read_back(store, &held, &seasonal.id).await.status,
        FactStatus::Archived,
        "…and the store kept it"
    );
}

/// 🚨 **A walk flags a link drawn by an archived claim, whichever way the
/// claim reached archived.**
///
/// Two roads to the same status: `retract` takes a claim back with no
/// replacement, and an ordinary `update_fact` marks a claim archived when a
/// later one replaces it. Before the two statuses collapsed, the walk's
/// marker only ever checked for the first — a link drawn by a claim archived
/// the second way came back unflagged, silently, which is the asymmetry the
/// collapse removes: one status, one check, both roads covered.
///
/// **Paired against a live claim in the same read**, because a marker that
/// fires on the archived attendee and stays off the live one is the only
/// shape that proves the check is reading the status rather than firing on
/// everything.
pub async fn a_walk_flags_a_link_drawn_by_a_claim_archived_through_an_ordinary_edit<M: Memory>(
    store: &M,
) {
    let party = EntityId("event:contract-archived-link-party".into());
    let moved_on = EntityId::person("person:contract-archived-link-moved-on");
    let still_going = EntityId::person("person:contract-archived-link-still-going");
    add(
        store,
        NewEntity::new(party.clone(), "The Archived Link Party", "the roster"),
    )
    .await;

    let mut attended = NewFact::about(moved_on.clone(), "went to the party", date(2026, 5, 1));
    attended.edge = Some(Edge::new(EdgeShape::Attendance, party.clone()));
    let archived_by_edit = capture(store, attended).await;
    edit(
        store,
        &archived_by_edit.address(),
        FactPatch {
            status: Some(FactStatus::Archived),
            ..Default::default()
        },
    )
    .await;

    let mut still_attends = NewFact::about(
        still_going.clone(),
        "is still going to the party",
        date(2026, 5, 1),
    );
    still_attends.edge = Some(Edge::new(EdgeShape::Attendance, party.clone()));
    capture(store, still_attends).await;

    let guests = graph::walk(
        store,
        &graph::GraphQuery {
            select: graph::Selection {
                subject: Some(party.clone()),
                ..graph::Selection::default()
            },
            follow: Some(graph::Follow {
                along: graph::Along::Edge(EdgeShape::Attendance),
                direction: Some(graph::Direction::In),
                ..graph::Follow::hop()
            }),
            ..graph::GraphQuery::default()
        },
    )
    .await
    .expect("a subject with a walk")
    .objects[0]
        .connected
        .clone();

    let via = |who: &EntityId| {
        guests
            .iter()
            .find(|o| &o.entity.id == who)
            .unwrap_or_else(|| panic!("{who} was not reached: {guests:?}"))
            .via
            .as_ref()
            .expect("a reached object says how the walk got to it")
    };
    assert!(
        via(&moved_on).retracted,
        "a link drawn by a claim archived through an ordinary edit was not flagged: {guests:?}",
    );
    assert!(
        !via(&still_going).retracted,
        "the live claim's link was flagged too, so the marker is not reading the status: \
         {guests:?}",
    );
}

/// **A declared type governs no write at all.**
///
/// A type is the QUERY vocabulary — how a caller asks which things answer a
/// shape. What may be taken off a thing is its KIND's question, and the two
/// were one function until this case existed: a thing that structurally
/// completed any declaration anybody had made became governed by it, with
/// nothing offered and nothing switched on.
///
/// **On its own this case proves nothing**, and it must be read beside
/// [`a_write_cannot_break_a_fit_that_already_exists`], which holds the
/// other half. Delete the fit guard outright and this one still passes —
/// "no write is refused" is exactly what a build with no guard does. Only
/// the kind half can tell the two apart.
///
/// A contract case rather than a domain one: what is under test is that a
/// STORE takes the write and the key is gone afterwards, which only a store
/// can answer for.
pub async fn a_declared_type_governs_no_write<M: Memory>(store: &M) {
    store
        .declare_type(DeclaredType::new(
            "contract-vocabulary",
            vec![
                Field::required("cost", ValueType::Number),
                Field::required("done_on", ValueType::Date),
            ],
        ))
        .await
        .expect("declaring a type of my own is accepted");

    // A THING carrying every key that type names. Its kind is `thing`, and
    // `thing` names no keys, so nothing here is governed however completely
    // it answers the vocabulary.
    let answers = EntityId("thing:contract-vocabulary-answerer".into());
    let costed = capture(
        store,
        NewFact {
            fields: [
                ("cost".to_string(), "40".to_string()),
                ("done_on".to_string(), "2026-04-18".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(answers.clone(), "the annual service", date(2026, 4, 18))
        },
    )
    .await;

    edit(
        store,
        &costed.address(),
        FactPatch {
            clear_fields: vec!["cost".to_string()],
            ..Default::default()
        },
    )
    .await;
    assert!(
        !read_back(store, &answers, &costed.id)
            .await
            .fields
            .contains_key("cost"),
        "a declared type is a vocabulary to ask with, so it takes nothing away from a \
         caller: the key goes",
    );
}

/// **A thing reads back as one dense row: its records' fields, folded.**
///
/// What a thing IS gets written down over several sittings, so the answer
/// to "what is this" is the fold and not the list of sittings. A caller
/// handed the records and left to fold them itself is a caller doing
/// jojobot's job.
///
/// **The row is there whether or not the records are**, which is the half
/// that cannot be tested against the records alone: a fold taken from the
/// shipped list would come back empty on exactly the call that asked for
/// the dense answer and nothing else.
///
/// **And it folds over ALL the thing's records rather than the kept ones.**
/// A filter chooses which objects come back; it does not change what each
/// one is.
pub async fn a_thing_reads_back_as_its_fields_folded<M: Memory>(store: &M) {
    let subject = EntityId("thing:contract-folded-thing".into());
    let sittings = [
        [("weight", "11"), ("wheel", "700c")],
        // The second sitting adds a key and writes one of the first
        // sitting's again — the newest write is what the thing holds now.
        [("weight", "12"), ("gears", "22")],
    ];
    for fields in sittings {
        capture(
            store,
            NewFact {
                fields: fields
                    .iter()
                    .map(|(key, value)| (key.to_string(), value.to_string()))
                    .collect(),
                ..NewFact::about(subject.clone(), "a sitting at the bench", date(2026, 4, 18))
            },
        )
        .await;
    }

    let dense = graph::walk(
        store,
        &graph::GraphQuery {
            select: graph::Selection {
                subject: Some(subject.clone()),
                ..graph::Selection::default()
            },
            include: graph::Include {
                facts: false,
                prose: false,
                stood_for: false,
            },
            follow: None,
            history: None,
        },
    )
    .await
    .expect("a handle is a selection")
    .objects;
    let object = &dense[0];
    assert_eq!(
        object.fields,
        [
            ("gears".to_string(), "22".to_string()),
            ("weight".to_string(), "12".to_string()),
            ("wheel".to_string(), "700c".to_string()),
        ]
        .into_iter()
        .collect(),
        "one row, one value per key, the newest write winning"
    );
    assert!(
        object.facts.is_empty(),
        "…and the records themselves were not asked for: {:?}",
        object.facts
    );

    // The other half, and the positive the negative above rests on: asking
    // for the records still gets every one of them, each addressed.
    let whole = graph::walk(
        store,
        &graph::GraphQuery {
            select: graph::Selection {
                subject: Some(subject.clone()),
                ..graph::Selection::default()
            },
            include: graph::Include {
                facts: true,
                prose: false,
                stood_for: false,
            },
            follow: None,
            history: None,
        },
    )
    .await
    .expect("a handle is a selection")
    .objects;
    assert_eq!(
        whole[0].facts.len(),
        2,
        "the sittings are still reachable: {:?}",
        whole[0].facts
    );
    assert_eq!(
        whole[0].fields, object.fields,
        "and the row does not change with them"
    );
    assert!(
        whole[0]
            .facts
            .iter()
            .all(|f| f.address().home == subject && f.address().local.ordinal().is_some()),
        "every record keeps the address that makes it editable: {:?}",
        whole[0].facts
    );
}

/// **What a thing holds is the newest WRITE of a key, never the newest
/// RECORD carrying it.**
///
/// The two orders agree until a key lives on more than one record and the
/// OLDER record is edited afterwards. Then the edit is the newest write of
/// that key on that thing while the record it landed in is still the older
/// one — and a fold that ranked records by their id answers with the value
/// the edit replaced. The agent edits a claim, is told it worked, reads it
/// back on the record and in the history, and every reader of "what the
/// thing IS" keeps serving the stale value.
pub async fn the_newest_write_wins_however_old_the_record_it_landed_in<M: Memory>(store: &M) {
    let subject = EntityId("thing:contract-edited-older-record".into());
    let older = capture(
        store,
        NewFact {
            fields: [("ridden_km".to_string(), "10".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(subject.clone(), "the first sitting", date(2026, 4, 18))
        },
    )
    .await;
    capture(
        store,
        NewFact {
            fields: [("ridden_km".to_string(), "20".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(subject.clone(), "the second sitting", date(2026, 4, 19))
        },
    )
    .await;
    let edited = edit(
        store,
        &older.address(),
        FactPatch {
            fields: [("ridden_km".to_string(), "30".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        },
    )
    .await;
    // The three surfaces that must agree, and the reason the third one
    // disagreeing is not a footnote: the record says the edit landed, the
    // history says it is the newest write, so a thing still holding the
    // value it replaced is the same data read two ways and answering twice.
    assert_eq!(
        edited.fields.get("ridden_km").map(String::as_str),
        Some("30"),
        "the edited record reads back changed, which is the surface"
    );
    assert_eq!(
        store
            .history(&subject, "ridden_km")
            .await
            .expect("history should succeed")
            .last()
            .map(|w| w.value.as_deref()),
        Some(Some("30")),
        "…and the edit is the newest write in the substrate"
    );
    assert_eq!(
        thing_fields(store, &subject)
            .await
            .get("ridden_km")
            .map(String::as_str),
        Some("30"),
        "…so it is what the thing holds, though its record was written first"
    );
}

/// **A cleared key stays cleared, and an older record does not put it
/// back.**
///
/// A clear is a write like any other — the newest one — so it takes the key
/// off the THING and not merely off the record that carried it. A fold
/// ranking records would find the key gone from the newest record, fall
/// back to an older one, and resurrect a value nobody wrote back.
pub async fn a_cleared_key_is_not_resurrected_by_an_older_record<M: Memory>(store: &M) {
    let subject = EntityId("thing:contract-cleared-key".into());
    capture(
        store,
        NewFact {
            fields: [("chain_wear".to_string(), "9".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(subject.clone(), "the first sitting", date(2026, 4, 18))
        },
    )
    .await;
    let newer = capture(
        store,
        NewFact {
            fields: [("chain_wear".to_string(), "11".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(subject.clone(), "the second sitting", date(2026, 4, 19))
        },
    )
    .await;
    edit(
        store,
        &newer.address(),
        FactPatch {
            clear_fields: vec!["chain_wear".to_string()],
            ..Default::default()
        },
    )
    .await;

    let held = thing_fields(store, &subject).await;
    assert!(
        !held.contains_key("chain_wear"),
        "the key was taken off the thing and stayed off: {held:?}"
    );
}

/// **A clear names what the ADDRESSED RECORD must not carry.** Naming a key
/// that record never carried changes nothing — not the record, and not the
/// thing.
///
/// The contract a caller reads says the patch describes the record, so an
/// agent tidying one record's keys has no reason to expect another
/// record's to move. A clear that reached past its address would take the
/// key off the THING while every surface the caller can see stayed
/// identical: the receipt is the edited record's own projection, which
/// never carried the key, and the record that did still reads it back on a
/// plain recall. Only the fold would know, and nothing told the caller to
/// look there.
pub async fn clearing_a_key_the_record_never_carried_changes_nothing<M: Memory>(store: &M) {
    let subject = EntityId("thing:contract-clear-off-address".into());
    capture(
        store,
        NewFact {
            fields: [("shipping_weight".to_string(), "10".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(subject.clone(), "weighed at the bench", date(2026, 4, 18))
        },
    )
    .await;
    let bare = capture(
        store,
        NewFact::about(subject.clone(), "rode it home", date(2026, 4, 19)),
    )
    .await;

    let receipt = edit(
        store,
        &bare.address(),
        FactPatch {
            clear_fields: vec!["shipping_weight".to_string()],
            ..Default::default()
        },
    )
    .await;

    assert!(
        !receipt.fields.contains_key("shipping_weight"),
        "the addressed record did not carry the key and still does not: {:?}",
        receipt.fields
    );
    assert_eq!(
        thing_fields(store, &subject)
            .await
            .get("shipping_weight")
            .map(String::as_str),
        Some("10"),
        "…and the record that DOES carry it keeps it on the thing"
    );
    assert_eq!(
        store
            .history(&subject, "shipping_weight")
            .await
            .expect("history should succeed")
            .last()
            .map(|w| w.value.as_deref()),
        Some(Some("10")),
        "…because a clear the record never needed wrote nothing at all"
    );
}

/// **A history longer than the window comes back cut, and says how much
/// exists.**
///
/// The read whose job is the small answer must not be able to flood the
/// caller it serves. The total is what makes the cut honest: it says how
/// many writes are there without handing them over.
///
/// Both ends of the rule in one case, because a cap that always fires is
/// indistinguishable from one that never does: a history inside the window
/// comes back whole, with nothing left out.
pub async fn a_long_history_is_cut_to_its_newest_and_says_how_many<M: Memory>(store: &M) {
    let subject = EntityId("thing:contract-long-history".into());
    let written = graph::WRITES_SHOWN + 5;
    for nth in 1..=written {
        capture(
            store,
            NewFact {
                fields: [("weight".to_string(), nth.to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(subject.clone(), "weighed at the bench", date(2026, 4, 18))
            },
        )
        .await;
    }
    let asking = |key: &str| {
        let key = key.to_string();
        let subject = subject.clone();
        async move {
            graph::walk(
                store,
                &graph::GraphQuery {
                    select: graph::Selection {
                        subject: Some(subject),
                        ..graph::Selection::default()
                    },
                    include: graph::Include {
                        facts: false,
                        prose: false,
                        stood_for: false,
                    },
                    follow: None,
                    history: Some(graph::History::of(&key)),
                },
            )
            .await
            .expect("a handle is a selection")
            .objects
        }
    };

    let found = asking("weight").await;
    let history = found[0]
        .history
        .as_ref()
        .expect("the query named a key, so the object carries its writes");
    assert_eq!(
        history.total, written,
        "the answer says how many writes exist"
    );
    assert_eq!(
        history.writes.len(),
        graph::WRITES_SHOWN,
        "…and hands back a window rather than all of them"
    );
    assert_eq!(history.elided(), 5, "…and how many it left out");
    assert_eq!(
        history
            .writes
            .iter()
            .map(|w| w.value.as_deref())
            .collect::<Vec<_>>(),
        (6..=written)
            .map(|nth| nth.to_string())
            .collect::<Vec<_>>()
            .iter()
            .map(|v| Some(v.as_str()))
            .collect::<Vec<_>>(),
        "the window is the NEWEST writes, still oldest first inside it"
    );

    // A key written a handful of times is untouched by any of this, and
    // says nothing was left out.
    let short = EntityId("thing:contract-short-history".into());
    for nth in 1..=3 {
        capture(
            store,
            NewFact {
                fields: [("cost".to_string(), nth.to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(short.clone(), "paid at the counter", date(2026, 4, 18))
            },
        )
        .await;
    }
    let found = graph::walk(
        store,
        &graph::GraphQuery {
            select: graph::Selection {
                subject: Some(short.clone()),
                ..graph::Selection::default()
            },
            include: graph::Include {
                facts: false,
                prose: false,
                stood_for: false,
            },
            follow: None,
            history: Some(graph::History::of("cost")),
        },
    )
    .await
    .expect("a handle is a selection")
    .objects;
    let history = found[0].history.as_ref().expect("the query named a key");
    assert_eq!((history.total, history.writes.len()), (3, 3));
    assert_eq!(
        history.elided(),
        0,
        "a history that fits leaves nothing out, and says so"
    );
}

/// 🚨 **A claim rewritten twice comes back field-for-field identical to one
/// written once, unless a walk that read facts also says how many writes
/// stand behind each of them.**
///
/// Paired with a claim nobody corrected: the once-written claim's count is
/// the case that fails on a build where every claim reads as "revised".
pub async fn a_read_of_facts_says_how_many_times_each_was_written<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-revision-count");
    let once = capture(
        store,
        NewFact::about(subject.clone(), "never touched again", date(2026, 4, 18)),
    )
    .await;
    let corrected = capture(
        store,
        NewFact::about(subject.clone(), "lent to Ralph", date(2026, 4, 18)),
    )
    .await;
    edit(
        store,
        &corrected.address(),
        FactPatch {
            content: Some("Ralph gave it back".into()),
            ..Default::default()
        },
    )
    .await;

    let found = graph::walk(
        store,
        &graph::GraphQuery {
            select: graph::Selection {
                subject: Some(subject.clone()),
                ..graph::Selection::default()
            },
            include: graph::Include {
                facts: true,
                prose: false,
                stood_for: false,
            },
            follow: None,
            history: None,
        },
    )
    .await
    .expect("a handle is a selection")
    .objects;

    let revisions = &found[0].fact_revisions;
    assert_eq!(
        revisions.get(&once.id),
        Some(&1),
        "a claim nobody corrected has one write: {revisions:?}"
    );
    assert_eq!(
        revisions.get(&corrected.id),
        Some(&2),
        "the corrected claim carries the count that says so: {revisions:?}"
    );
}

/// **No flag of its own.** A walk that never asked for facts has none on
/// its object either, so there is nothing to count — the signal follows
/// `include.facts` rather than asking a caller to name it twice, which
/// would be the extra argument rule 155 exists to refuse.
pub async fn a_walk_with_no_facts_carries_no_revision_counts<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-no-facts-no-counts");
    capture(
        store,
        NewFact::about(subject.clone(), "said once", date(2026, 4, 18)),
    )
    .await;

    let found = graph::walk(
        store,
        &graph::GraphQuery {
            select: graph::Selection {
                subject: Some(subject.clone()),
                ..graph::Selection::default()
            },
            include: graph::Include {
                facts: false,
                prose: false,
                stood_for: false,
            },
            follow: None,
            history: None,
        },
    )
    .await
    .expect("a handle is a selection")
    .objects;

    assert!(
        found[0].fact_revisions.is_empty(),
        "a walk that did not ask for facts carries no revision counts either: {:?}",
        found[0].fact_revisions
    );
}

/// 🚨 **The batched read of a whole entity's history agrees with the
/// per-record read, fact for fact.**
///
/// One entity holding a claim nobody corrected and one corrected once —
/// the same pairing the per-fact case above uses, so a build that only
/// gets the single-write case right cannot pass both.
pub async fn claim_histories_agrees_with_claim_history_per_fact<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-claim-histories");
    let once = capture(
        store,
        NewFact::about(subject.clone(), "said once", date(2026, 4, 18)),
    )
    .await;
    let corrected = capture(
        store,
        NewFact::about(subject.clone(), "lent to ralph", date(2026, 4, 18)),
    )
    .await;
    edit(
        store,
        &corrected.address(),
        FactPatch {
            content: Some("ralph gave it back".into()),
            ..Default::default()
        },
    )
    .await;

    let batched = store
        .claim_histories(&subject)
        .await
        .expect("claim_histories should succeed");
    assert_eq!(
        batched.len(),
        2,
        "one entry per fact on the entity: {batched:?}"
    );

    let words_of = |id: &FactId| -> Vec<String> {
        batched
            .get(id)
            .unwrap_or_else(|| panic!("{id:?} missing from the batched result: {batched:?}"))
            .iter()
            .map(|w| w.content.clone())
            .collect()
    };
    assert_eq!(words_of(&once.id), vec!["said once".to_string()]);
    assert_eq!(
        words_of(&corrected.id),
        vec![
            "lent to ralph".to_string(),
            "ralph gave it back".to_string()
        ],
        "oldest first, agreeing with claim_history's own order",
    );

    // **The negative that gives it meaning.** An entity nobody wrote is a
    // miss, exactly as claim_history and recall answer one — not an empty
    // map, which would read as "written and holding nothing".
    let ghost = EntityId::person("person:contract-claim-histories-ghost");
    let missing = store.claim_histories(&ghost).await;
    assert!(
        matches!(missing, Err(MemoryError::UnknownEntity { .. })),
        "an entity nobody created is a miss, not an empty history: {missing:?}"
    );
}

/// about the store rather than about the enum, and only a store can answer
/// it. The negative is the filter: a pet is not returned by a listing of
/// things, so the kind is carried rather than defaulted to something.
pub async fn a_pet_is_its_own_kind_in_the_store<M: Memory>(store: &M) {
    let cat = EntityId("pet:contract-pet-cat".into());
    ensure(store, &cat).await;

    let read = store
        .list_entities(Some(EntityKind::PET))
        .await
        .expect("a listing of pets");
    let found = read
        .iter()
        .find(|e| e.id == cat)
        .expect("the pet comes back from a listing of its own kind");
    assert_eq!(
        found.kind,
        EntityKind::PET,
        "and it reads back as a pet rather than as whatever a store defaults to: {found:?}",
    );

    let things = store
        .list_entities(Some(EntityKind::THING))
        .await
        .expect("a listing of things");
    assert!(
        !things.iter().any(|e| e.id == cat),
        "a pet is not a thing, and the store's own filter agrees: {things:?}",
    );
}

/// **A rhythm is a noun of its own, and it is refused without a parent.**
///
/// The parent answers whose job the loop is: a maintenance loop sits under
/// the thing maintained, a review loop under the bot that carries it. A
/// rhythm nobody owns is a modelling failure rather than a valid shape, so
/// the store never holds one.
///
/// Both halves, because either alone passes on the wrong build. Without the
/// refusal, nothing enforces the rule; without the write that succeeds, the
/// case passes identically on a build that refuses every rhythm there is.
pub async fn a_rhythm_is_refused_without_a_parent<M: Memory>(store: &M) {
    let owner = EntityId("thing:contract-kettle".into());
    let orphan = EntityId("rhythm:contract-descale".into());
    add(
        store,
        NewEntity::new(owner.clone(), "Contract Kettle", "contract-fixture"),
    )
    .await;

    let refused = store
        .add_entity(NewEntity::new(
            orphan.clone(),
            "Descale The Kettle",
            "contract-fixture",
        ))
        .await
        .expect_err("a rhythm under nothing is refused");
    assert!(
        matches!(refused, MemoryError::InvalidEntity(_)),
        "the shape is wrong, so it is the entity that is invalid: {refused:?}",
    );
    assert!(
        !store
            .list_entities(Some(EntityKind::RHYTHM))
            .await
            .expect("a listing of rhythms")
            .iter()
            .any(|e| e.id == orphan),
        "a refused rhythm is not in the store",
    );

    // The positive the verdict rests on: the same write with a parent
    // lands, so the refusal above is about the parent and not about the
    // kind being unwritable.
    let held = add(
        store,
        NewEntity {
            parent: Some(owner.clone()),
            ..NewEntity::new(orphan.clone(), "Descale The Kettle", "contract-fixture")
        },
    )
    .await;
    assert_eq!(held.parent.as_ref(), Some(&owner));
    assert_eq!(held.kind, EntityKind::RHYTHM);
}

/// 🚨 **A duplicate that got past the guard is repairable, and the repair
/// is legible afterwards.**
///
/// One thing split across two handles answers half of every question and
/// reports the half as whole — and it is invisible from every entry in it,
/// because each handle looks like a complete record of itself. **With a
/// repair the guard only has to be good; without one it has to be
/// perfect.**
///
/// ⭐ **The half-answer is asserted BEFORE the fold**, so the whole answer
/// afterwards is the fold's doing and not the fixture's.
///
/// ⛔️ **The store does not ask whether the two really were duplicates.**
/// That is the operator's judgement. What the store owes is that the act is
/// recorded and readable from the thing it happened to.
pub async fn folding_a_duplicate_makes_the_split_answer_whole<M: Memory>(store: &M) {
    let kept = EntityId::person("person:contract-folded-kept");
    let spare = EntityId::person("person:contract-folded-spare");
    add(
        store,
        NewEntity::new(kept.clone(), "Kept", "contract-fixture"),
    )
    .await;
    add(
        store,
        NewEntity::new(spare.clone(), "Spare", "contract-fixture"),
    )
    .await;

    capture(
        store,
        NewFact::about(kept.clone(), "plays the bass", date(2026, 5, 1)),
    )
    .await;
    capture(
        store,
        NewFact::about(spare.clone(), "reads the sleeve notes", date(2026, 5, 2)),
    )
    .await;

    // **The fault, stated as an assertion.** Each handle answers with its
    // own half and nothing on either says the other half exists.
    let before = store.recall(&kept).await.expect("the kept side reads");
    assert_eq!(
        before.len(),
        1,
        "the fixture did not split the thing, so the fold below proves nothing: {before:?}",
    );
    assert!(
        !before.iter().any(|f| f.content == "reads the sleeve notes"),
        "the halves were not apart to begin with: {before:?}",
    );

    let folded = store
        .merge(
            &spare,
            &kept,
            Some("one person, filed twice"),
            date(2026, 5, 3),
        )
        .await
        .expect("the fold lands");

    // **Whole from the survivor.** Both halves, in one read.
    let after = store.recall(&kept).await.expect("the survivor reads");
    assert!(
        after.iter().any(|f| f.content == "plays the bass")
            && after.iter().any(|f| f.content == "reads the sleeve notes"),
        "the fold did not bring the other half across: {after:?}",
    );
    assert_eq!(folded.rehomed, 1, "the fold miscounted what it moved");

    // **The account is on the survivor and names what was folded.** A merge
    // nobody can read afterwards is the one outcome this verb must not
    // leave behind.
    assert_eq!(folded.record.subject, kept);
    // 🚨 **The literal, not the constant.** Asserting through `MERGED_FROM`
    // compares the code with itself: rename the constant and both sides
    // move together, so a stored key could be changed under everybody with
    // the suite still green. **A key is DATA** — every record already
    // written carries the old spelling — so the spelling is the thing to
    // pin.
    assert_eq!(
        MERGED_FROM, "merged_from",
        "the stored key changed spelling; every record already written carries the old one",
    );
    assert_eq!(
        folded.record.fields.get(MERGED_FROM).map(String::as_str),
        Some(spare.as_str()),
        "the account did not name the handle it folded away: {:?}",
        folded.record,
    );
    assert!(
        after.iter().any(|f| f.content == "one person, filed twice"),
        "the reason is not readable from the thing it happened to: {after:?}",
    );

    // **The folded handle still resolves and says where it went.** It is
    // not deleted, and it is not a live second thing either.
    let held = store
        .list_entities(None)
        .await
        .expect("the roster reads")
        .into_iter()
        .find(|e| e.id == spare)
        .expect("the folded row is still there — nothing here is deleted");
    assert_eq!(
        held.merged_into.as_ref(),
        Some(&kept),
        "the folded row does not forward, so it is a husk that answers half",
    );

    // **Neither side may be folded twice.** A chain stops answering in one
    // hop, so it is refused rather than followed.
    let again = store
        .merge(&spare, &kept, None, date(2026, 5, 4))
        .await
        .expect_err("a folded row is not a side to fold");
    assert!(
        matches!(again, MemoryError::AlreadyMerged { .. }),
        "refolding a forwarding row was not refused as such: {again:?}",
    );

    // And nothing folds into itself.
    let itself = store
        .merge(&kept, &kept, None, date(2026, 5, 4))
        .await
        .expect_err("a fold has two sides");
    assert!(
        matches!(itself, MemoryError::NothingToMerge { .. }),
        "folding a thing into itself was not refused as such: {itself:?}",
    );
}

/// **A fact address minted before a fold still resolves its handle, and
/// now says where the claim went** — never a bare miss indistinguishable
/// from an address that never existed.
///
/// The fold moves every claim off the folded row and renumbers it landing
/// on the survivor, so the pre-fold local id answers nothing under the
/// folded row anymore. The folded row is still real — `Entity::merged_into`
/// says so — so the honest refusal is `AlreadyMerged`, naming the
/// survivor, on every verb that resolves a `FactAddress` down to one claim.
///
/// **Paired with a genuinely unknown address**, so this case cannot be
/// told apart from a miss that was never anything else — those must read
/// differently, or the new sentence is really the old one wearing new
/// prose.
pub async fn a_stale_address_after_a_fold_says_where_it_went<M: Memory>(store: &M) {
    let kept = EntityId::person("person:contract-relic-survivor");
    let spare = EntityId::person("person:contract-relic-forwarded");
    add(
        store,
        NewEntity::new(kept.clone(), "Relic Survivor", "contract-fixture"),
    )
    .await;
    add(
        store,
        NewEntity::new(spare.clone(), "Relic Forwarded", "contract-fixture"),
    )
    .await;

    let written = capture(
        store,
        NewFact::about(spare.clone(), "minted before the fold", date(2026, 5, 10)),
    )
    .await;
    let stale = written.address();

    store
        .merge(&spare, &kept, None, date(2026, 5, 11))
        .await
        .expect("the fold lands");

    let edit = store
        .update_fact(&stale, FactPatch::default())
        .await
        .expect_err("a stale address must not silently succeed or silently miss");
    assert!(
        matches!(&edit, MemoryError::AlreadyMerged { attempted, into }
            if attempted == &spare.to_string() && into == &kept.to_string()),
        "update_fact's refusal must name the survivor: {edit:?}",
    );

    let history = store
        .claim_history(&stale)
        .await
        .expect_err("claim_history must answer the same way update_fact does");
    assert!(
        matches!(&history, MemoryError::AlreadyMerged { .. }),
        "claim_history did not recognise the same fold: {history:?}",
    );

    let retracted = store
        .retract(&stale, None, date(2026, 5, 11))
        .await
        .expect_err("retract must answer the same way");
    assert!(
        matches!(&retracted, MemoryError::AlreadyMerged { .. }),
        "retract did not recognise the same fold: {retracted:?}",
    );

    // **Paired: a genuinely unknown address still misses, and misses
    // differently** — an address under a live entity that never held
    // this local id.
    let never = FactAddress::new(kept.clone(), FactId("f999".into()));
    let miss = store
        .update_fact(&never, FactPatch::default())
        .await
        .expect_err("an address nobody ever wrote must still miss");
    assert!(
        matches!(&miss, MemoryError::UnknownFact { .. }),
        "a genuinely unknown address must not read as a fold: {miss:?}",
    );
}

/// 🚨 **A fold moves claims, so every lineage pointer at one moves with
/// it — everywhere a reader can see it, not just where the fold happens to
/// look.**
///
/// A claim's address is its home and its number, and a fold changes both.
/// Anything that said "I was worked out from THAT" is now naming an address
/// nobody can reach: the reader gets a pointer, follows it, and finds
/// nothing — a claim that has stopped being able to say where it came from,
/// which is worse than one that never said, because it reads as if it did.
///
/// ⭐ **The pointer is FOLLOWED rather than compared.** The case never works
/// out where the claim ought to have landed; it takes the address the store
/// hands back and asks the store for the claim at it. A fold that renumbered
/// differently is still correct here, and a fix that pointed every claim at
/// the survivor's newest row is not.
///
/// **The pair is in the same read**: a pointer that was already right stays
/// exactly where it was, and a claim that rests on nothing still rests on
/// nothing. Without them, rewriting every pointer to the survivor would pass.
pub async fn a_fold_carries_the_lineage_that_points_at_what_it_moved<M: Memory>(store: &M) {
    let folded = EntityId::person("person:contract-quagmire");
    let survivor = EntityId::person("person:contract-towelie");
    let onlooker = EntityId::person("person:contract-gene");
    for (id, name) in [
        (&folded, "Contract Quagmire"),
        (&survivor, "Contract Towelie"),
        (&onlooker, "Contract Gene"),
    ] {
        add(store, NewEntity::new(id.clone(), name, "contract-fixture")).await;
    }

    // The claim the fold will move, and one on the survivor that it will
    // not: the second is what makes the pair possible.
    let moves = capture(
        store,
        NewFact::about(
            folded.clone(),
            "the ferry left from the north pier",
            date(2026, 6, 1),
        ),
    )
    .await;
    let stays = capture(
        store,
        NewFact::about(
            survivor.clone(),
            "the bridge is shut on Sundays",
            date(2026, 6, 1),
        ),
    )
    .await;

    // Three claims on a third thing, which the fold does not touch at all:
    // one resting on the claim that moves, one on the claim that stays, one
    // resting on nothing.
    let built = capture(
        store,
        NewFact {
            derived_from: Some(moves.address()),
            ..NewFact::about(
                onlooker.clone(),
                "so the crossing got longer",
                date(2026, 6, 2),
            )
        },
    )
    .await;
    let steady = capture(
        store,
        NewFact {
            derived_from: Some(stays.address()),
            ..NewFact::about(
                onlooker.clone(),
                "so Sunday is the slow day",
                date(2026, 6, 2),
            )
        },
    )
    .await;
    let alone = capture(
        store,
        NewFact::about(
            onlooker.clone(),
            "the timetable is on the wall",
            date(2026, 6, 2),
        ),
    )
    .await;

    // **Stated as an assertion before the fold**, so a red below is the
    // fold's doing and not a fixture that never resolved in the first place.
    let source_was = stays.address();
    assert!(
        claim_at(store, &moves.address()).await.is_some(),
        "the lineage did not resolve before the fold, so this case is about the fixture",
    );

    store
        .merge(
            &folded,
            &survivor,
            Some("one person, filed twice"),
            date(2026, 6, 3),
        )
        .await
        .expect("the fold lands");

    let after = store
        .recall(&onlooker)
        .await
        .expect("the onlooker still reads");
    let held = |id: &FactId| {
        after
            .iter()
            .find(|f| &f.id == id)
            .unwrap_or_else(|| panic!("the fold lost a claim it never touched (id {id})"))
            .clone()
    };

    // 🚨 **The pointer at the moved claim resolves.**
    let carried = held(&built.id)
        .derived_from
        .expect("the fold dropped the lineage instead of moving it");
    let source = claim_at(store, &carried).await.unwrap_or_else(|| {
        panic!(
            "a claim's lineage names {carried}, where no claim is: the fold moved what it \
             pointed at and left the pointer behind",
        )
    });
    assert_eq!(
        source.content, moves.content,
        "the lineage resolves, but to some other claim than the one it was built on",
    );

    // **The pair, in the same read.** A pointer that was already right is
    // left alone, and a claim resting on nothing still rests on nothing —
    // so a fold that aimed every pointer at the survivor fails here.
    let untouched = held(&steady.id)
        .derived_from
        .expect("a lineage the fold had no business touching was dropped");
    assert_eq!(
        untouched, source_was,
        "a lineage that already resolved was rewritten by a fold that did not move it",
    );
    assert_eq!(
        claim_at(store, &untouched).await.map(|f| f.content),
        Some(stays.content.clone()),
        "…and it no longer resolves to the claim it always named",
    );
    assert_eq!(
        held(&alone.id).derived_from,
        None,
        "a claim that rests on nothing was given a lineage by a fold",
    );

    // **And the same pointer read through the chain**, which is the other
    // reader that can see one. A store that fixed the served claim and not
    // its writes would answer these two differently.
    let chain = store
        .claim_history(&built.address())
        .await
        .expect("the chain reads");
    let newest = chain.last().expect("a claim has at least one write");
    let written = newest
        .derived_from
        .clone()
        .expect("the newest write dropped the lineage the claim still carries");
    assert!(
        claim_at(store, &written).await.is_some(),
        "the chain says a claim was built on {written}, where no claim is",
    );
}

/// 🚨 **A fold's raw-handle sweep matches the folded side's CURRENT handle
/// only — it never chases a handle the folded side answered to before a
/// rename — so a pointer written under an earlier spelling stays one hop
/// short of the survivor.**
///
/// A rename rewrites nothing (rule 243): a child's `parent`, an edge's
/// `object` and a record's `refs` entry keep whatever handle was current
/// when they were written. `merge` already repoints these for the handle
/// it was given — this proves it also repoints a pointer wearing a FORMER
/// handle of the folded side.
///
/// **Paired, and not optional**: the same three pointers, drawn again
/// after the rename so they already wear the current handle. A fix that
/// only chased former handles and stopped sweeping the direct case would
/// pass the first half of this case and fail the second.
pub async fn a_fold_repoints_a_pointer_wearing_a_former_handle_of_the_folded_side<M: Memory>(
    store: &M,
) {
    let before = EntityId::person("person:contract-fold-rename-before");
    let after = EntityId::person("person:contract-fold-rename-after");
    let survivor = EntityId::person("person:contract-fold-rename-survivor");
    let onlooker = EntityId::person("person:contract-fold-rename-onlooker");
    add(
        store,
        NewEntity::new(before.clone(), "Fold Rename Before", "contract-fixture"),
    )
    .await;
    add(
        store,
        NewEntity::new(survivor.clone(), "Fold Rename Survivor", "contract-fixture"),
    )
    .await;
    add(
        store,
        NewEntity::new(onlooker.clone(), "Fold Rename Onlooker", "contract-fixture"),
    )
    .await;

    // A child parented, and an edge and a ref drawn, all at the folded
    // side's handle BEFORE it is renamed — the pointer this case is about.
    let child_former = EntityId("thing:contract-fold-rename-child-former".into());
    add(
        store,
        NewEntity {
            parent: Some(before.clone()),
            ..NewEntity::new(
                child_former.clone(),
                "Fold Rename Child Former",
                "contract-fixture",
            )
        },
    )
    .await;
    let edge_former = capture(
        store,
        NewFact {
            edge: Some(Edge::new(EdgeShape::Connection, before.clone())),
            refs: vec![before.clone()],
            ..NewFact::about(
                onlooker.clone(),
                "drawn before the rename",
                date(2026, 7, 1),
            )
        },
    )
    .await;

    store
        .rename_entity(&before, &after, None, date(2026, 7, 2), None)
        .await
        .expect("the rename lands")
        .written()
        .expect("nothing collides with the destination");

    // The paired half: the same three pointers, drawn AFTER the rename,
    // already wearing the current handle.
    let child_current = EntityId("thing:contract-fold-rename-child-current".into());
    add(
        store,
        NewEntity {
            parent: Some(after.clone()),
            ..NewEntity::new(
                child_current.clone(),
                "Fold Rename Child Current",
                "contract-fixture",
            )
        },
    )
    .await;
    let edge_current = capture(
        store,
        NewFact {
            edge: Some(Edge::new(EdgeShape::Connection, after.clone())),
            refs: vec![after.clone()],
            ..NewFact::about(onlooker.clone(), "drawn after the rename", date(2026, 7, 3))
        },
    )
    .await;

    store
        .merge(&after, &survivor, None, date(2026, 7, 4))
        .await
        .expect("the fold lands");

    // **`parent`**: both children are under the survivor, whichever
    // handle their pointer was written under.
    let kids = store
        .children(&survivor)
        .await
        .expect("children reads under the survivor");
    assert!(
        kids.contains(&child_former),
        "a child parented on a FORMER handle of the folded side did not follow the \
         fold: {kids:?}",
    );
    assert!(
        kids.contains(&child_current),
        "a child parented on the folded side's CURRENT handle did not follow the \
         fold: {kids:?}",
    );

    // **`edge.object` and `refs`**: read back off the onlooker, both must
    // now name the survivor directly.
    let held = store
        .recall(&onlooker)
        .await
        .expect("the onlooker still reads");
    let fact = |id: &FactId| {
        held.iter()
            .find(|f| &f.id == id)
            .unwrap_or_else(|| panic!("the fold lost a claim it never touched (id {id})"))
    };
    let former_fact = fact(&edge_former.id);
    assert_eq!(
        former_fact.edge.as_ref().map(|e| &e.object),
        Some(&survivor),
        "an edge drawn at a FORMER handle of the folded side did not follow the fold: \
         {former_fact:?}",
    );
    assert!(
        former_fact.refs.contains(&survivor),
        "a ref naming a FORMER handle of the folded side did not follow the fold: \
         {former_fact:?}",
    );
    let current_fact = fact(&edge_current.id);
    assert_eq!(
        current_fact.edge.as_ref().map(|e| &e.object),
        Some(&survivor),
        "an edge drawn at the folded side's CURRENT handle did not follow the fold: \
         {current_fact:?}",
    );
    assert!(
        current_fact.refs.contains(&survivor),
        "a ref naming the folded side's CURRENT handle did not follow the fold: \
         {current_fact:?}",
    );
}

/// **A session-kind handle is refused by `add_entity`, never written.**
///
/// `validate_write_subject` already refuses a memory write onto a session's
/// handle at six other call sites — this is the seventh, and the one that
/// stops the row from existing in the first place. Without it, a session-kind
/// entity `add_entity` creates is permanently inert the moment it lands: every
/// gated verb refuses to touch it again, and it is indistinguishable in the
/// store from a genuine session except by trying to write to it.
pub async fn add_entity_refuses_a_session_kind_handle<M: Memory>(store: &M) {
    let attempted = EntityId("session:contract-add-entity-session-gap".into());
    let err = store
        .add_entity(NewEntity::new(
            attempted.clone(),
            "Contract Session Gap",
            "contract-fixture",
        ))
        .await
        .expect_err("a session-kind handle must be refused by add_entity, not written");
    assert!(
        matches!(&err, MemoryError::InvalidSubject(message) if message.contains("session")),
        "expected InvalidSubject naming the session, got {err:?}",
    );
}

/// **A session-kind handle is refused on either side of a merge.**
///
/// `merge` moves rows directly rather than through `capture`, so it never
/// passed through the gate the other six call sites share. The check runs
/// before either side's existence is resolved — a fold naming a session is
/// wrong by its kind alone, whether or not a row for that handle exists.
pub async fn merge_refuses_a_session_kind_handle_on_either_side<M: Memory>(store: &M) {
    let ordinary = add(
        store,
        NewEntity::new(
            EntityId::person("person:contract-merge-session-gap-ordinary"),
            "Contract Merge Session Gap Ordinary",
            "contract-fixture",
        ),
    )
    .await;
    let session = EntityId("session:contract-merge-session-gap".into());

    let folded_err = store
        .merge(&session, &ordinary.id, None, date(2026, 5, 3))
        .await
        .expect_err("a session named as the folded side must be refused, not merged away");
    assert!(
        matches!(&folded_err, MemoryError::InvalidSubject(message) if message.contains("session")),
        "expected InvalidSubject naming the session on the folded side, got {folded_err:?}",
    );

    let survivor_err = store
        .merge(&ordinary.id, &session, None, date(2026, 5, 3))
        .await
        .expect_err("a session named as the survivor must be refused, not written into");
    assert!(
        matches!(&survivor_err, MemoryError::InvalidSubject(message) if message.contains("session")),
        "expected InvalidSubject naming the session on the survivor side, got {survivor_err:?}",
    );
}

pub async fn run_all<M: Memory>(store: &M) {
    capture_reads_back(store).await;
    preserves_all_fields(store).await;
    a_claim_can_say_nothing_about_when_the_thing_happened(store).await;
    a_claims_span_covers_every_day_it_ran(store).await;
    happened_through_with_no_start_is_refused(store).await;
    a_patch_may_widen_a_standing_start_but_not_orphan_one(store).await;
    the_day_a_thing_happened_is_versioned_like_the_rest_of_the_claim(store).await;
    derived_from_must_name_a_fact_that_exists(store).await;
    derived_from_on_an_edit_must_name_a_fact_that_exists(store).await;
    pipe_in_content_round_trips(store).await;
    a_backslash_in_content_round_trips(store).await;
    both_provenances_survive(store).await;
    edge_whitespace_is_normalized(store).await;
    multiple_facts_all_recallable(store).await;
    subjects_are_isolated(store).await;
    malicious_subjects_are_rejected(store).await;
    recall_unknown_is_a_miss_not_an_empty_page(store).await;

    every_kind_holds_facts(store).await;

    a_claim_carries_when_it_was_taken_in(store).await;
    folding_a_duplicate_makes_the_split_answer_whole(store).await;
    a_fold_carries_the_lineage_that_points_at_what_it_moved(store).await;
    a_fold_repoints_a_pointer_wearing_a_former_handle_of_the_folded_side(store).await;
    a_claims_lineage_is_walkable_from_its_source(store).await;
    a_folded_value_says_who_backs_it(store).await;
    a_summed_key_has_no_backing_to_report(store).await;
    a_machine_read_claim_names_what_it_was_read_from(store).await;
    referring_to_answers_from_the_far_end(store).await;
    a_child_names_its_parent_and_reads_back(store).await;
    children_are_handles_and_one_level_deep(store).await;
    a_write_that_rewrites_a_child_leaves_it_where_it_was(store).await;
    a_parent_that_is_not_a_handle_is_refused_before_the_guard(store).await;
    children_of_an_unknown_entity_is_a_miss(store).await;
    an_unnamed_parent_is_refused_and_provisions_nothing(store).await;
    nothing_may_be_its_own_parent(store).await;

    prose_is_replaced_whole_and_reads_back(store).await;
    add_entity_reads_back(store).await;
    list_entities_filters_by_kind(store).await;
    update_entity_edits_metadata_in_place(store).await;
    update_entity_screens_a_colliding_rename(store).await;
    update_entity_screens_a_colliding_alias(store).await;
    update_entity_is_not_blocked_by_its_own_labels(store).await;
    update_entity_does_not_re_screen_the_handle(store).await;
    update_entity_without_a_rename_is_not_screened(store).await;
    update_entity_unknown_handle_never_creates(store).await;
    a_second_rename_of_a_stale_handle_reports_where_it_went(store).await;
    add_entity_keeps_its_alternate_names(store).await;
    add_entity_screens_every_name_an_entity_answers_to(store).await;

    capture_writes_an_edge_that_reads_back(store).await;
    every_edge_shape_reads_back(store).await;
    a_wrong_kind_edge_object_is_refused(store).await;
    an_edge_object_is_screened_by_the_guard(store).await;
    update_fact_attaches_an_edge(store).await;
    update_fact_sets_and_clears_a_field(store).await;

    a_key_written_many_times_holds_one_value_and_counts(store).await;
    a_counter_totals_its_writes_and_keeps_them(store).await;
    an_edit_appends_and_the_value_it_replaced_stays_in_the_history(store).await;
    clearing_a_key_leaves_its_writes_behind(store).await;
    history_of_an_unwritten_key_is_empty_and_of_no_entity_is_a_miss(store).await;
    a_correction_keeps_what_the_claim_used_to_say(store).await;
    claim_history_of_no_record_is_a_miss_and_of_no_entity_is_an_entity_miss(store).await;
    each_write_of_a_claim_records_its_own_moment(store).await;

    a_records_fields_survive_capture(store).await;
    a_records_ref_is_screened_by_the_guard(store).await;
    a_reserved_field_key_is_refused(store).await;
    retracting_a_record_marks_it_and_records_why(store).await;
    a_retraction_is_one_way(store).await;
    the_retraction_marker_is_not_one_of_the_things_fields(store).await;
    clearing_the_retraction_marker_is_refused(store).await;
    a_retraction_needs_no_reason(store).await;
    retracting_an_unknown_address_never_writes(store).await;

    facts_carry_a_usable_address(store).await;
    update_fact_edits_in_place(store).await;
    an_edit_can_carry_a_new_day_and_omitted_leaves_it_alone(store).await;
    a_refutation_is_an_ordinary_content_edit(store).await;
    promotion_to_testimony_needs_confirmation(store).await;
    demotion_to_inference_is_free(store).await;

    a_hedged_claim_round_trips(store).await;
    standing_defaults_to_what_the_provenance_implies(store).await;
    a_capture_declares_its_own_standing(store).await;
    settling_a_hedge_needs_confirmation_and_keeps_its_provenance(store).await;
    reopening_a_settled_claim_needs_no_ceremony(store).await;
    a_patch_moves_only_the_axis_it_names(store).await;
    update_fact_unknown_address_never_creates(store).await;
    update_fact_tells_an_unknown_handle_from_an_empty_entity(store).await;

    capture_requires_an_existing_subject(store).await;
    capture_requires_an_existing_edge_object(store).await;
    update_fact_requires_an_existing_edge_object(store).await;
    a_rename_and_a_recreated_handle_does_not_hijack_an_edge(store).await;
    a_rename_and_a_recreated_handle_does_not_hijack_a_ref(store).await;
    a_rename_and_a_recreated_handle_does_not_hijack_a_parent(store).await;
    malformed_entity_fields_are_rejected(store).await;
    a_cross_link_takes_the_task_layers_own_grammar(store).await;
    a_field_at_the_validators_limit_survives_storage(store).await;

    a_declared_type_reads_back(store).await;
    declaring_a_type_again_replaces_its_keys(store).await;
    a_type_with_no_keys_is_refused_and_writes_nothing(store).await;
    a_shipped_type_refuses_a_callers_redeclaration(store).await;
    two_stored_types_may_name_one_key(store).await;
    a_stored_type_matches_a_record_that_never_declared_it(store).await;

    a_graph_query_selects_a_kind_and_returns_its_prose(store).await;
    a_graph_query_filters_on_a_stored_value_and_walks_an_edge(store).await;
    a_walk_marks_a_link_whose_claim_the_store_took_back(store).await;
    a_documents_id_is_not_the_handle_and_survives_a_rewrite(store).await;
    a_value_is_found_without_naming_the_key_it_is_under(store).await;
    a_rewrite_can_take_the_edge_off_and_leaves_it_alone_otherwise(store).await;
    a_declared_reference_key_is_walkable_against_the_store(store).await;
    a_trip_records_who_came_and_answers_from_either_end(store).await;
    the_kinds_are_rows_and_a_shipped_one_is_closed(store).await;
    an_owned_kind_this_build_dropped_is_reclaimed(store).await;
    a_pet_is_its_own_kind_in_the_store(store).await;
    a_rhythm_is_refused_without_a_parent(store).await;
    a_thing_reads_back_as_its_fields_folded(store).await;
    the_newest_write_wins_however_old_the_record_it_landed_in(store).await;
    a_cleared_key_is_not_resurrected_by_an_older_record(store).await;
    clearing_a_key_the_record_never_carried_changes_nothing(store).await;
    a_reference_keeps_the_kind_it_points_at(store).await;
    a_write_cannot_put_a_value_the_type_refuses(store).await;
    a_type_named_after_a_kind_does_not_gate_that_kinds_writes(store).await;
    neither_half_writes_over_the_others_keys(store).await;
    a_closed_set_refuses_a_write_outside_it(store).await;
    a_reference_must_name_an_entity_that_exists(store).await;
    a_write_cannot_break_a_fit_that_already_exists(store).await;
    a_supersede_that_breaks_a_fit_is_refused_and_a_retraction_is_not(store).await;
    a_walk_flags_a_link_drawn_by_a_claim_archived_through_an_ordinary_edit(store).await;
    a_declared_type_governs_no_write(store).await;
    a_long_history_is_cut_to_its_newest_and_says_how_many(store).await;
    a_read_of_facts_says_how_many_times_each_was_written(store).await;
    a_walk_with_no_facts_carries_no_revision_counts(store).await;
    claim_histories_agrees_with_claim_history_per_fact(store).await;

    archive_entity_persists_the_reason_and_the_moment(store).await;
    a_second_archive_is_refused_not_overwritten(store).await;

    add_entity_refuses_a_session_kind_handle(store).await;
    merge_refuses_a_session_kind_handle_on_either_side(store).await;
}

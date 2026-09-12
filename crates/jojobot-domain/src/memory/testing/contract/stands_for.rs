use super::support::{capture, edit};
use super::*;

/// **A mark names claims, and each has to be there** — the same rule
/// [`derived_from_must_name_a_fact_that_exists`] enforces, just plural: a
/// mark that took any well-formed address without looking would let a
/// record stand for a claim that never existed.
///
/// Both halves, in one read: a mark naming what exists reads back off the
/// record, and a mark naming what does not is refused with the addresses
/// that do exist.
pub async fn stands_for_must_name_facts_that_exist<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-stands-for-basic");
    let alpha = capture(
        store,
        NewFact::about(subject.clone(), "drinks oat milk", date(2026, 5, 1)),
    )
    .await;
    let beta = capture(
        store,
        NewFact::about(subject.clone(), "drinks oat milk too", date(2026, 5, 2)),
    )
    .await;

    // Named and there: accepted, and the mark reads back off the record.
    let marked = edit(
        store,
        &alpha.address(),
        FactPatch {
            stands_for: Some(vec![beta.address()]),
            ..FactPatch::default()
        },
    )
    .await;
    assert_eq!(
        marked.stands_for,
        vec![beta.address()],
        "a mark naming a claim that exists survives the write"
    );

    // Named and absent: refused, with the addresses that do exist.
    let missing = FactAddress::parse("person:contract-stands-for-basic#f99").expect("well-formed");
    let refused = store
        .update_fact(
            &alpha.address(),
            FactPatch {
                stands_for: Some(vec![missing.clone()]),
                ..FactPatch::default()
            },
        )
        .await;
    let Err(MemoryError::UnknownFact { attempted, nearest }) = &refused else {
        panic!("a mark naming no claim must be refused as a miss, got {refused:?}");
    };
    assert_eq!(attempted, &missing.to_string());
    assert!(
        nearest.contains(&beta.address().to_string()),
        "the addresses that DO exist are what makes it repairable: {nearest:?}"
    );

    // A home nobody has heard of is an entity miss, not a fact miss — the
    // same shape `retract` and `capture` already answer with.
    let nowhere = FactAddress::parse("person:contract-stands-for-nobody#f1").expect("well-formed");
    let refused = store
        .update_fact(
            &alpha.address(),
            FactPatch {
                stands_for: Some(vec![nowhere]),
                ..FactPatch::default()
            },
        )
        .await;
    assert!(
        matches!(refused, Err(MemoryError::UnknownEntity { .. })),
        "a mark whose home is unknown is an entity miss, got {refused:?}"
    );

    // Nothing was written by either refusal: the mark from the accepted edit
    // still reads back exactly as it did before both refused attempts.
    let facts = store.recall(&subject).await.expect("recall should succeed");
    let still = facts
        .iter()
        .find(|f| f.id == alpha.id)
        .expect("the marked record is still there");
    assert_eq!(
        still.stands_for,
        vec![beta.address()],
        "a refused edit leaves the mark as it was: {still:?}"
    );
}

/// **① and ②, paired in the same read: a real mark reads back as a mark, and
/// an ordinary field wearing its name does not.**
///
/// The mark is a dedicated field, structurally apart from the fields bag —
/// so a caller writing an ordinary field named `stands_for` gets exactly
/// that: an ordinary field, inert, never read as a citation. Only the
/// dedicated path on [`FactPatch`] produces the real thing. Testing the
/// refusal alone would pass on a build where nothing counts as a mark;
/// testing the acceptance alone would pass on a build where an ordinary
/// field IS one. Both, together, are what closes the gap.
pub async fn an_ordinary_field_of_the_same_name_is_not_a_mark<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-stands-for-decoy");
    let source = capture(
        store,
        NewFact::about(subject.clone(), "said the tide was low", date(2026, 5, 3)),
    )
    .await;

    // An ordinary field named "stands_for" is just a field.
    let decoy = capture(
        store,
        NewFact {
            fields: [("stands_for".to_string(), source.address().to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                subject.clone(),
                "so the pier was reachable",
                date(2026, 5, 4),
            )
        },
    )
    .await;
    assert!(
        decoy.stands_for.is_empty(),
        "a fields-bag entry named 'stands_for' must not be read as the mark: {decoy:?}"
    );
    assert_eq!(
        decoy.fields.get("stands_for").map(String::as_str),
        Some(source.address().to_string()).as_deref(),
        "it is still an ordinary field, kept as written"
    );

    // The dedicated path, on the same record, produces the real thing.
    let real = edit(
        store,
        &decoy.address(),
        FactPatch {
            stands_for: Some(vec![source.address()]),
            ..FactPatch::default()
        },
    )
    .await;
    assert_eq!(
        real.stands_for,
        vec![source.address()],
        "the dedicated field is the only path that produces a real mark"
    );

    // **The check PM asked to be RUN rather than reasoned about: with the
    // decoy field and the real mark both present on the same record at
    // once, does a read still tell them apart?** It does, structurally —
    // they are two different fields on [`Fact`], never one rendered blob —
    // and this is the read that proves it rather than assumes it.
    assert_eq!(
        real.fields.get("stands_for").map(String::as_str),
        Some(source.address().to_string()).as_deref(),
        "the decoy field is untouched by the real mark landing beside it"
    );
    assert_eq!(
        real.stands_for,
        vec![source.address()],
        "and the real mark is untouched by the decoy field sitting beside it"
    );
}

/// **③: the count never overstates.** A mark that stands for nothing is the
/// one outcome this refusal exists to make impossible — see
/// [`validate_stands_for`]. Sabotage target: what does a read return when the
/// source set is empty? A build that stored `Some(vec![])` as a mark would
/// let a session interrupted right after starting one read back as a mark
/// with no sources, indistinguishable from a real, deliberate empty fold —
/// which does not exist.
pub async fn a_mark_standing_for_nothing_is_refused<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-stands-for-empty");
    let record = capture(
        store,
        NewFact::about(subject.clone(), "a plain claim", date(2026, 5, 5)),
    )
    .await;

    let refused = store
        .update_fact(
            &record.address(),
            FactPatch {
                stands_for: Some(Vec::new()),
                ..FactPatch::default()
            },
        )
        .await;
    assert!(
        matches!(refused, Err(MemoryError::InvalidFact(_))),
        "an empty set must be refused, never stored as a mark standing for nothing: {refused:?}"
    );

    let facts = store.recall(&subject).await.expect("recall should succeed");
    let still = facts
        .iter()
        .find(|f| f.id == record.id)
        .expect("the record is still there");
    assert!(
        still.stands_for.is_empty(),
        "a refused write leaves the record exactly as it was: {still:?}"
    );
}

/// **A record cannot be marked as standing for itself.** Naming its own
/// address says nothing about any other claim, and would let a walk that
/// ever followed the mark loop back on the record it started from.
pub async fn a_record_cannot_stand_for_itself<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-stands-for-self");
    let record = capture(
        store,
        NewFact::about(subject.clone(), "a claim about itself", date(2026, 5, 6)),
    )
    .await;

    let refused = store
        .update_fact(
            &record.address(),
            FactPatch {
                stands_for: Some(vec![record.address()]),
                ..FactPatch::default()
            },
        )
        .await;
    assert!(
        matches!(refused, Err(MemoryError::InvalidFact(_))),
        "a record naming its own address in its own mark must be refused: {refused:?}"
    );
}

/// **One layer, so the pile is always one step away.** A record already
/// marked as standing for others is itself a shape, and a shape cannot be
/// folded into another mark — checked from the naming side, which is the same
/// property stated the other way: a shape may not name a source that is
/// itself a shape.
pub async fn a_shape_cannot_name_a_source_that_is_itself_a_shape<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-stands-for-stack");
    let leaf_one = capture(
        store,
        NewFact::about(subject.clone(), "the first leaf", date(2026, 5, 17)),
    )
    .await;
    let leaf_two = capture(
        store,
        NewFact::about(subject.clone(), "the second leaf", date(2026, 5, 18)),
    )
    .await;

    // leaf_one now stands for leaf_two: leaf_one is a shape.
    edit(
        store,
        &leaf_one.address(),
        FactPatch {
            stands_for: Some(vec![leaf_two.address()]),
            ..FactPatch::default()
        },
    )
    .await;

    // A third record tries to fold the shape into its own mark.
    let outer = capture(
        store,
        NewFact::about(subject.clone(), "the outer claim", date(2026, 5, 19)),
    )
    .await;
    let refused = store
        .update_fact(
            &outer.address(),
            FactPatch {
                stands_for: Some(vec![leaf_one.address()]),
                ..FactPatch::default()
            },
        )
        .await;
    assert!(
        matches!(refused, Err(MemoryError::InvalidFact(_))),
        "naming a record that is already a shape must be refused: {refused:?}"
    );

    // Nothing was written: the outer record still stands for nothing.
    let facts = store.recall(&subject).await.expect("recall should succeed");
    let still = facts
        .iter()
        .find(|f| f.id == outer.id)
        .expect("the outer record is still there");
    assert!(
        still.stands_for.is_empty(),
        "a refused edit leaves the mark as it was: {still:?}"
    );
}

/// **A mark may not name the same source twice.** The count is the number of
/// distinct sources, so a repeat would overstate it — the same property
/// [`a_mark_standing_for_nothing_is_refused`] guards from the other side.
pub async fn a_mark_cannot_repeat_the_same_source<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-stands-for-repeat");
    let record = capture(
        store,
        NewFact::about(subject.clone(), "the standing claim", date(2026, 5, 7)),
    )
    .await;
    let source = capture(
        store,
        NewFact::about(subject.clone(), "one of the duplicates", date(2026, 5, 8)),
    )
    .await;

    let refused = store
        .update_fact(
            &record.address(),
            FactPatch {
                stands_for: Some(vec![source.address(), source.address()]),
                ..FactPatch::default()
            },
        )
        .await;
    assert!(
        matches!(refused, Err(MemoryError::InvalidFact(_))),
        "naming the same source twice in one mark must be refused: {refused:?}"
    );
}

/// **`clear_stands_for` takes the mark off**, and cleared-before-set is the
/// order every such pair on [`FactPatch`] uses: a caller naming both meant
/// the mark it named.
pub async fn clear_stands_for_takes_the_mark_off<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-stands-for-clear");
    let record = capture(
        store,
        NewFact::about(subject.clone(), "the standing claim", date(2026, 5, 9)),
    )
    .await;
    let source = capture(
        store,
        NewFact::about(subject.clone(), "a duplicate", date(2026, 5, 10)),
    )
    .await;

    let marked = edit(
        store,
        &record.address(),
        FactPatch {
            stands_for: Some(vec![source.address()]),
            ..FactPatch::default()
        },
    )
    .await;
    assert_eq!(marked.stands_for, vec![source.address()]);

    let cleared = edit(
        store,
        &record.address(),
        FactPatch {
            clear_stands_for: true,
            ..FactPatch::default()
        },
    )
    .await;
    assert!(
        cleared.stands_for.is_empty(),
        "clear_stands_for must leave the record standing for nothing named: {cleared:?}"
    );

    // Naming both in one patch: the clear happens first, so a caller naming
    // both meant the set it named — never a no-op and never an empty write.
    let other = capture(
        store,
        NewFact::about(subject.clone(), "another duplicate", date(2026, 5, 11)),
    )
    .await;
    let both = edit(
        store,
        &record.address(),
        FactPatch {
            clear_stands_for: true,
            stands_for: Some(vec![other.address()]),
            ..FactPatch::default()
        },
    )
    .await;
    assert_eq!(
        both.stands_for,
        vec![other.address()],
        "clear-then-set leaves the set that was named, not an empty mark"
    );
}

/// **Setting the mark again replaces it whole — it does not merge.** This is
/// what makes "several calls" honest: a session that names `[a, b]` and is
/// then interrupted before a third call names `[a, b, c]` reads back exactly
/// two sources, never three and never a merge of both attempts into
/// something wider than either call named.
pub async fn stands_for_replaces_rather_than_merges<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-stands-for-replace");
    let record = capture(
        store,
        NewFact::about(subject.clone(), "the standing claim", date(2026, 5, 12)),
    )
    .await;
    let first = capture(
        store,
        NewFact::about(subject.clone(), "first duplicate", date(2026, 5, 13)),
    )
    .await;
    let second = capture(
        store,
        NewFact::about(subject.clone(), "second duplicate", date(2026, 5, 14)),
    )
    .await;

    edit(
        store,
        &record.address(),
        FactPatch {
            stands_for: Some(vec![first.address(), second.address()]),
            ..FactPatch::default()
        },
    )
    .await;

    // A later call names only one of the two — an interrupted session, or a
    // deliberate narrowing. Either way, the record reads back with exactly
    // what this call named.
    let narrowed = edit(
        store,
        &record.address(),
        FactPatch {
            stands_for: Some(vec![first.address()]),
            ..FactPatch::default()
        },
    )
    .await;
    assert_eq!(
        narrowed.stands_for,
        vec![first.address()],
        "a later set replaces the mark whole; it must not merge with the earlier one"
    );
}

/// **The sources stay active and readable.** Marking a record as standing
/// for others changes nothing about them: no status flip, no move, no
/// retraction — each stays its own row at its own address, independently
/// walkable, exactly as the operator's ruling requires.
pub async fn sources_stay_active_and_walkable_after_being_marked<M: Memory>(store: &M) {
    let subject = EntityId::person("person:contract-stands-for-untouched");
    let shape = capture(
        store,
        NewFact::about(subject.clone(), "the standing claim", date(2026, 5, 15)),
    )
    .await;
    let source = capture(
        store,
        NewFact::about(
            subject.clone(),
            "a duplicate, kept in full",
            date(2026, 5, 16),
        ),
    )
    .await;

    edit(
        store,
        &shape.address(),
        FactPatch {
            stands_for: Some(vec![source.address()]),
            ..FactPatch::default()
        },
    )
    .await;

    let facts = store.recall(&subject).await.expect("recall should succeed");
    let still = facts
        .iter()
        .find(|f| f.id == source.id)
        .expect("the source is still its own, independently readable record");
    assert_eq!(
        still.status,
        FactStatus::Active,
        "a source's status must not change just because it was named in a mark"
    );
    assert_eq!(
        still.content, source.content,
        "a source's own words are untouched by being named in a mark"
    );
}

/// Every spec above, in one call — see [`base::run_all`].
pub async fn run_all_stands_for<M: Memory>(store: &M) {
    stands_for_must_name_facts_that_exist(store).await;
    an_ordinary_field_of_the_same_name_is_not_a_mark(store).await;
    a_mark_standing_for_nothing_is_refused(store).await;
    a_record_cannot_stand_for_itself(store).await;
    a_shape_cannot_name_a_source_that_is_itself_a_shape(store).await;
    a_mark_cannot_repeat_the_same_source(store).await;
    clear_stands_for_takes_the_mark_off(store).await;
    stands_for_replaces_rather_than_merges(store).await;
    sources_stay_active_and_walkable_after_being_marked(store).await;
}

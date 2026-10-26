//! What `walk` and `walk_in_window` read from a store, proved through the fake's read counters.

use super::*;

/// **A browse by one kind asks the store for that kind, and answers as the whole
/// listing would have.** The store builds every entity's row for a whole listing,
/// and a walk that wants one kind and nothing else has no use for the rest. The
/// cases that need the whole listing still ask for it: a named handle (it is
/// resolved and screened against every entity), a walk (it reaches any kind) and
/// a query naming no kind.
///
/// The answer is held against what the fixture holds, so a walk that listed less
/// and answered less fails the second half: the objects, what archival and a
/// fold left out, what another identity's runs were counted as, and the children
/// of one parent all come back as they did.
#[tokio::test]
async fn a_kind_only_walk_asks_the_store_for_that_kind_and_answers_as_before() {
    let store = crate::memory::testing::InMemoryMemory::booted();
    for (handle, name, parent) in [
        ("place:springfield", "Springfield", None),
        ("place:quahog", "Quahog", None),
        ("place:contract-atlas", "Atlas", None),
        ("place:gorge-spot", "Gorge Spot", Some("place:springfield")),
        ("place:far-country", "Far Country", None),
        ("person:bart", "Bart", None),
        ("person:kenny", "Kenny", None),
    ] {
        let mut new = NewEntity::new(EntityId(handle.into()), name, "user-named");
        new.parent = parent.map(|p| EntityId(p.into()));
        store.add_entity(new).await.expect("the fixture is written");
    }
    store
        .archive_entity(
            &EntityId("place:far-country".into()),
            "a mistaken write",
            None,
        )
        .await
        .expect("the place is archived");
    // **A thing folded into another keeps its row**, which a merge leaves behind;
    // staged here because the fixture has no merge to run.
    store.past_the_guard(Entity {
        merged_into: Some(EntityId("place:contract-atlas".into())),
        ..entity("place:wharf-road", "Wharf Road")
    });
    let strangers_run = a_run("bot:delta", "contract-delta-run", "delta's own", "a beat");

    let asked = |select: Selection, follow: Option<Follow>| selecting(select).following(follow);
    let places = || Selection {
        kind: Some(EntityKind::PLACE),
        asked_by: Some(EntityId("bot:gamma".into())),
        ..Selection::default()
    };

    // The browse: that kind alone is asked for.
    let before = store.listed_kinds().len();
    let browsed = walk(
        &store,
        std::slice::from_ref(&strangers_run),
        &asked(places(), None),
    )
    .await
    .expect("a kind selects");
    assert_eq!(
        store.listed_kinds()[before..],
        [Some(EntityKind::PLACE)],
        "the store is asked for places and for nothing else"
    );
    assert_eq!(
        handles(&browsed.objects),
        vec![
            "place:contract-atlas",
            "place:gorge-spot",
            "place:quahog",
            "place:springfield"
        ],
        "every browsable place and no person, no run"
    );
    assert_eq!(
        browsed.archived_excluded, 1,
        "the archived place is counted"
    );
    assert_eq!(browsed.merged_excluded, 1, "the folded place is counted");
    assert_eq!(
        browsed.withheld, 0,
        "another bot's run is no place, so it is not counted against a place browse"
    );

    // The parent narrows the same set.
    let before = store.listed_kinds().len();
    let children = walk(
        &store,
        &[],
        &asked(
            Selection {
                parent: Some(EntityId("place:springfield".into())),
                ..places()
            },
            None,
        ),
    )
    .await
    .expect("a kind and a parent select");
    assert_eq!(store.listed_kinds()[before..], [Some(EntityKind::PLACE)]);
    assert_eq!(handles(&children.objects), vec!["place:gorge-spot"]);

    // The sessions the caller hands in join the listing, and a run is found by
    // its own kind: asked for runs, the store is asked for runs, and the run the
    // caller owns is there while the other bot's is counted.
    let own_run = a_run("bot:gamma", "contract-gamma-run", "gamma's own", "a beat");
    let before = store.listed_kinds().len();
    let runs = walk(
        &store,
        &[own_run, strangers_run.clone()],
        &asked(
            Selection {
                kind: Some(EntityKind::SESSION),
                asked_by: Some(EntityId("bot:gamma".into())),
                ..Selection::default()
            },
            None,
        ),
    )
    .await
    .expect("runs select");
    assert_eq!(store.listed_kinds()[before..], [Some(EntityKind::SESSION)]);
    assert_eq!(handles(&runs.objects), vec!["session:contract-gamma-run"]);
    assert_eq!(
        runs.withheld, 1,
        "the other bot's run is counted, not shown"
    );

    // The whole listing, wherever the walk may reach past one kind.
    let named = Selection {
        subject: Some(EntityId("place:springfield".into())),
        ..places()
    };
    let unkinded = Selection {
        parent: Some(EntityId("place:springfield".into())),
        asked_by: Some(EntityId("bot:gamma".into())),
        ..Selection::default()
    };
    let following = Follow::hop();
    for (what, query) in [
        ("a named handle", asked(named, None)),
        ("a walk", asked(places(), Some(following))),
        ("a query naming a parent and no kind", asked(unkinded, None)),
    ] {
        let before = store.listed_kinds().len();
        walk(&store, &[], &query).await.expect(what);
        assert_eq!(
            store.listed_kinds()[before..],
            [None],
            "{what} needs every entity, so the whole listing is asked for"
        );
    }
}

/// **A browse reads the objects its window will serve and stops there.** Sixty
/// places each hold a record; the room has space for 35 of them at 100 apiece.
/// The read takes the 36th, the first that overflows, and not the 24 behind it,
/// so the store is asked for 36 objects' records and not 60. What it did not read
/// is counted: none before the window and 24 after it.
///
/// Paired with the same browse read in full: the objects it did read are the ones
/// the full read has in the same places, field for field, so reading less never
/// meant reading differently. And with an offset the objects before it are not
/// read at all.
#[tokio::test]
async fn a_windowed_browse_reads_what_fits_and_counts_what_it_did_not() {
    let store = crate::memory::testing::InMemoryMemory::booted();
    let names: Vec<EntityId> = (0..60)
        .map(|i| EntityId::new(EntityKind::PLACE, format!("window-{i:02}")))
        .collect();
    for id in &names {
        // Staged past the near-miss screen, which would hold sixty numbered
        // names against each other.
        store.past_the_guard(entity(id.as_str(), id.slug()));
        store
            .capture(crate::memory::NewFact::about(
                id.clone(),
                "was visited",
                jiff::civil::date(2026, 10, 1),
            ))
            .await
            .expect("the record is written");
    }
    let query = browse(EntityKind::PLACE);
    let cost = |_: &Object| 100usize;
    let window = |offset: usize| Window {
        offset,
        budget: 3_500,
        size_of: &cost,
    };

    let before = store.record_reads();
    let read = walk_in_window(&store, &[], &query, Some(window(0)))
        .await
        .expect("a kind selects");
    assert_eq!(
        store.record_reads() - before,
        36,
        "the 35 that fit and the one that overflowed are read, and no other"
    );
    assert_eq!(read.objects.len(), 36);
    assert_eq!((read.skipped, read.left_over), (0, 24));
    assert_eq!(
        handles(&read.objects)[..2],
        [names[0].as_str(), names[1].as_str()]
    );

    let before = store.record_reads();
    let full = walk(&store, &[], &query).await.expect("a kind selects");
    assert_eq!(
        store.record_reads() - before,
        60,
        "the read in full asks for every object, which is what the window saves"
    );
    assert_eq!((full.skipped, full.left_over), (0, 0));
    for (windowed, whole) in read.objects.iter().zip(&full.objects) {
        assert_eq!(
            format!("{windowed:?}"),
            format!("{whole:?}"),
            "an object read in a window is the object the full read has"
        );
    }
    assert_eq!(
        (read.withheld, read.archived_excluded, read.merged_excluded),
        (full.withheld, full.archived_excluded, full.merged_excluded),
        "what the selection counted does not depend on what was read"
    );

    let before = store.record_reads();
    let later = walk_in_window(&store, &[], &query, Some(window(10)))
        .await
        .expect("a kind selects");
    assert_eq!(
        store.record_reads() - before,
        36,
        "the ten before the offset are not read"
    );
    assert_eq!((later.skipped, later.left_over), (10, 14));
    assert_eq!(handles(&later.objects)[0], names[10].as_str());
}

/// **A windowed browse in order reads the front of the order and no more.** Sixty
/// places each hold a size; many tie, one holds a word where a number goes, and
/// one holds none. The head of the ordering names the first dozen and the ties
/// of the last, and a window the room fits inside it reads those objects only.
///
/// Paired: a window that reaches past a head that was cut is not served from it.
/// It reads what it always read, and its objects are the full read's, in the same
/// places, for a room that runs past the head and for an offset that starts
/// beyond it.
#[tokio::test]
async fn an_ordered_window_is_served_from_the_head_of_the_order_and_never_past_it() {
    use crate::memory::filter::{Direction, Order};
    use crate::memory::types::{DeclaredType, Field, ValueType};
    let store = crate::memory::testing::InMemoryMemory::booted();
    store
        .declare_type(
            DeclaredType::new("place", vec![Field::new("size", ValueType::Number)]),
            None,
        )
        .await
        .expect("declared");
    let names: Vec<EntityId> = (0..60)
        .map(|i| EntityId::new(EntityKind::PLACE, format!("window-{i:02}")))
        .collect();
    for (i, id) in names.iter().enumerate() {
        store.past_the_guard(entity(id.as_str(), id.slug()));
        let size = match i {
            13 => "many".to_string(),
            41 => String::new(),
            _ => (i % 9).to_string(),
        };
        let mut fact = crate::memory::NewFact::about(
            id.clone(),
            "was visited",
            jiff::civil::date(2026, 10, 1),
        );
        if i != 41 {
            fact.fields.insert("size".into(), size);
            fact.sets.insert("size".into());
        }
        store.capture(fact).await.expect("the record is written");
    }
    let mut query = selecting(Selection {
        kind: Some(EntityKind::PLACE),
        order: Some(Order::new("size", Direction::Ascending)),
        ..Selection::default()
    });
    let full = walk(&store, &[], &query).await.expect("a kind selects");
    assert_eq!(full.objects.len(), 60);
    assert_eq!(
        handles(&full.objects)[..3],
        [names[0].as_str(), names[9].as_str(), names[18].as_str()],
        "the full read places the smallest first, and a tie in the order it came in"
    );
    assert_eq!(
        handles(&full.objects)[58..],
        [names[13].as_str(), names[41].as_str()],
        "a word where a number goes and no value at all come last"
    );
    let head = store
        .ordered_first_k(
            "size",
            ValueType::Number,
            Direction::Ascending,
            Some(EntityKind::PLACE),
            None,
            12,
        )
        .await
        .expect("a head");
    assert!(
        head.cut,
        "{} of 58 placed things stand in the head",
        head.head.len()
    );
    assert!(head.head.len() >= 12, "the ties of the last are not cut");
    query.select.ordered_head = Some(head.clone());

    let cost = |_: &Object| 100usize;
    let window = |offset: usize, budget: usize| Window {
        offset,
        budget,
        size_of: &cost,
    };
    let same_as_full = |served: &Selected, from: usize| {
        let served: Vec<&str> = handles(&served.objects);
        let whole: Vec<&str> = handles(&full.objects[from..from + served.len()]);
        assert_eq!(served, whole, "the objects from {from}, in order");
    };

    // Five fit, and the sixth is the one that overflows.
    let before = store.record_reads();
    let inside = walk_in_window(&store, &[], &query, Some(window(0, 500)))
        .await
        .expect("a kind selects");
    assert_eq!(
        store.record_reads() - before,
        6,
        "the five that fit and the one that overflowed, and no other"
    );
    assert_eq!(inside.objects.len(), 6);
    assert_eq!((inside.skipped, inside.left_over), (0, 54));
    same_as_full(&inside, 0);

    // Past the head: a room for forty, and an offset beyond the head.
    for (offset, budget) in [(0, 4_000), (head.head.len() + 3, 500)] {
        let before = (
            store.record_reads(),
            store.age_reads(),
            store.claim_history_reads(),
        );
        let past = walk_in_window(&store, &[], &query, Some(window(offset, budget)))
            .await
            .expect("a kind selects");
        let read = (
            store.record_reads() - before.0,
            store.age_reads() - before.1,
            store.claim_history_reads() - before.2,
        );
        assert!(
            read.0 > head.head.len(),
            "the window at {offset} for {budget} goes past the head, so it reads as it always did"
        );
        assert_eq!(
            read,
            (60, 60, 60),
            "the window at {offset} for {budget} reads each of the sixty things once, whether or \
             not the head served some of them first"
        );
        assert_eq!(
            handles(&past.objects),
            handles(&full.objects),
            "answered in full, as a question the window cannot serve is"
        );
        assert_eq!((past.skipped, past.left_over), (0, 0));
    }

    // A head that was not cut is every placed thing, and the rest follow in the
    // order they came in: the whole order is served.
    let whole_head = store
        .ordered_first_k(
            "size",
            ValueType::Number,
            Direction::Ascending,
            Some(EntityKind::PLACE),
            None,
            100,
        )
        .await
        .expect("a head");
    assert!(!whole_head.cut);
    query.select.ordered_head = Some(whole_head);
    let all = walk_in_window(&store, &[], &query, Some(window(0, 100_000)))
        .await
        .expect("a kind selects");
    assert_eq!(all.objects.len(), 60);
    same_as_full(&all, 0);
}

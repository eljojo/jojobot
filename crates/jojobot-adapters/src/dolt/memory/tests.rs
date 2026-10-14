use super::*;
use crate::dolt::tests::{Scratch, free_port};
use crate::dolt::{Dolt, migrate};
use crate::testing::migrated_database;
use jiff::civil::date;
use jojobot_domain::memory::{FactPatch, NewEntity, NewFact};

/// 🚨 **The substrate and the claim's own row answer the same thing**, on a
/// claim written once and on a claim corrected twice.
///
/// ⛔️ **This is what makes the switch that follows a no-op rather than a
/// leap.** The projection lands before anything reads it precisely so the
/// two can be held against each other first: a store where they disagree is
/// one where moving the reads changes answers, and nobody would know which
/// answers.
///
/// **Both shapes, because they fail differently.** A claim with one write
/// is the case the backfill produces and the commonest thing in the store;
/// a claim with three is the case the projection exists for, and a
/// projection that took the OLDEST write would pass the first and fail the
/// second.
#[tokio::test]
async fn the_projection_and_the_row_agree() {
    let scratch = Scratch::new("projection");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "projection")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    // The kinds, or no handle parses and every write is refused.
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");

    let subject = EntityId::person("person:contract-projected");
    memory
        .add_entity(NewEntity::new(
            subject.clone(),
            "Projected",
            "contract-fixture",
        ))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the guard waves it through");
    let once = memory
        .capture(NewFact::about(
            subject.clone(),
            "written once and left alone",
            date(2026, 8, 10),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("the guard waves it through");
    let corrected = memory
        .capture(NewFact::about(
            subject.clone(),
            "first thing said",
            date(2026, 8, 10),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("the guard waves it through");
    for said in ["second thing said", "third thing said"] {
        memory
            .update_fact(
                &corrected.address(),
                FactPatch {
                    content: Some(said.into()),
                    provenance: Some(Provenance::Inference),
                    ..Default::default()
                },
                &EntityId("bot:sigma".into()),
            )
            .await
            .expect("update_fact ok")
            .written()
            .expect("the guard waves it through");
    }

    let mut tx = pool.begin().await.expect("a transaction");
    // **Both these low-level readers want the storage key**, not the
    // handle a caller sees — resolved here for the same reason every
    // other lookup at this level resolves one first.
    let (key, _) = memory
        .resolve(&mut tx, &subject)
        .await
        .expect("resolve ok")
        .expect("the subject exists");
    let off_the_row = memory.facts_of(&mut tx, &key).await.expect("the rows read");
    let projected = memory
        .facts_projected(&mut tx, &key)
        .await
        .expect("the substrate projects");
    assert_eq!(
        projected, off_the_row,
        "the substrate and the row disagree, so moving the reads would change answers",
    );

    let one = memory
        .fact_projected(&mut tx, &FactAddress::new(key.clone(), once.id.clone()))
        .await
        .expect("the substrate projects one");
    assert_eq!(
        one.as_ref().map(|f| f.content.as_str()),
        Some("written once and left alone"),
        "a claim with one write did not project as itself",
    );
    let many = memory
        .fact_projected(
            &mut tx,
            &FactAddress::new(key.clone(), corrected.id.clone()),
        )
        .await
        .expect("the substrate projects one");
    assert_eq!(
        many.as_ref().map(|f| f.content.as_str()),
        Some("third thing said"),
        "the projection took a write that is not the newest",
    );
    tx.commit().await.expect("the read commits");

    store.stop().await;
}

/// 🚨 **A claim written thousands of times is read by its address in time
/// that follows its writes, not their square.**
///
/// A role holder renews its lease on every write, so one claim collects a write
/// per call, and every renewal reads it back. The statements that picked a
/// claim's newest write asked for it with a `MAX` correlated to each row, which
/// the store evaluated once per write the claim has: 2.5 s at 2,000 writes. No
/// case read a claim with more than a few writes, so nothing noticed.
///
/// The writes are seeded by one bulk insert, because making them through the
/// verbs would take as long as the defect. The newest carries the only
/// `written_at`, so a read that picked an older write also answers wrong, and
/// the bound is two seconds against milliseconds, wide enough for a loaded
/// machine and well under what the correlated statements take.
#[tokio::test]
async fn a_claim_written_thousands_of_times_is_read_by_its_address_in_linear_time() {
    const WRITES: usize = 3000;
    let scratch = Scratch::new("hot-claim");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "hotclaim")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");

    let subject = EntityId::person("person:milhouse");
    memory
        .add_entity(NewEntity::new(
            subject.clone(),
            "Milhouse",
            "contract-fixture",
        ))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the guard waves it through");
    let claim = memory
        .capture(NewFact::about(
            subject.clone(),
            "write 1",
            date(2026, 10, 7),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("the guard waves it through");

    let mut tx = pool.begin().await.expect("a transaction");
    let (key, _) = memory
        .resolve(&mut tx, &subject)
        .await
        .expect("resolve ok")
        .expect("the subject exists");
    let mut batch: Vec<String> = Vec::new();
    for ordinal in 2..=WRITES {
        let written_at = if ordinal == WRITES {
            "'2026-10-07T12:00:00Z'"
        } else {
            "NULL"
        };
        batch.push(format!(
            "('{}','{}',{ordinal},'write {ordinal}','inference','active','2026-10-07',{written_at})",
            key.as_str(),
            claim.id.as_str()
        ));
        if batch.len() == 500 || ordinal == WRITES {
            sqlx::query(&format!(
                "INSERT INTO fact_write (entity, fact_id, ordinal, content, provenance, status, \
                 recorded_at, written_at) VALUES {}",
                batch.join(",")
            ))
            .execute(&mut *tx)
            .await
            .expect("the writes are seeded");
            batch.clear();
        }
    }

    let address = FactAddress::new(key.clone(), claim.id.clone());
    let started = std::time::Instant::now();
    let read = memory
        .fact_projected(&mut tx, &address)
        .await
        .expect("the substrate projects one");
    let one_read = started.elapsed();
    assert_eq!(
        read.as_ref().map(|f| f.content.as_str()),
        Some(format!("write {WRITES}").as_str()),
        "the read took a write that is not the newest",
    );
    assert!(
        one_read < std::time::Duration::from_secs(2),
        "reading one claim written {WRITES} times took {one_read:?}",
    );

    let started = std::time::Instant::now();
    let touched = DoltMemory::touched_moments(&mut tx, &key, std::slice::from_ref(&claim.id))
        .await
        .expect("the moments read");
    let touch_read = started.elapsed();
    assert_eq!(
        touched.get(&claim.id).map(ToString::to_string).as_deref(),
        Some("2026-10-07T12:00:00Z"),
        "the moment is not the newest write's",
    );
    assert!(
        touch_read < std::time::Duration::from_secs(2),
        "reading the moment of one claim written {WRITES} times took {touch_read:?}",
    );
    tx.commit().await.expect("the read commits");

    store.stop().await;
}

/// **The collision path, actually watched.** `Draw` and `open_drawing`
/// exist, by their own doc comments, so a test can supply a draw that
/// collides on demand — entropy will not produce a collision on its own.
/// Nothing had ever called either: this is that test.
///
/// A rigged draw hands back an already-taken badge for its first two
/// calls, then a fresh one. The second entity must still land, must wear
/// a badge distinct from the one it collided with, and the draw must
/// have been asked more than once — proving retry happened rather than
/// merely being possible.
#[tokio::test]
async fn a_drawn_badge_that_collides_retries_until_one_is_free() {
    let scratch = Scratch::new("badge-collision-retry");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "badgecollisionretry")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");

    let memory = DoltMemory::open(pool.clone());
    let first = EntityId::person("person:contract-badge-collision-holder");
    memory
        .add_entity(NewEntity::new(first.clone(), "First", "contract-fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the guard waves it through");

    let mut tx = pool.begin().await.expect("a transaction");
    let known = memory.known(&mut tx).await.expect("the known rows read");
    tx.commit().await.expect("the read commits");
    let taken = known
        .iter()
        .find(|e| e.id == first)
        .and_then(|e| e.badge.clone())
        .expect("the first entity was badged");

    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let rigged: Draw = {
        let calls = calls.clone();
        let taken = taken.clone();
        std::sync::Arc::new(move || {
            let n = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if n < 2 {
                taken.clone()
            } else {
                ids::drawing()()
            }
        })
    };

    let colliding = DoltMemory::open_drawing(pool.clone(), rigged);
    let second = EntityId::person("person:contract-badge-collision-retried");
    colliding
        .add_entity(NewEntity::new(second.clone(), "Second", "contract-fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the guard waves it through");

    assert!(
        calls.load(std::sync::atomic::Ordering::SeqCst) >= 3,
        "the draw must be asked again after each collision, not just once"
    );

    let mut tx = pool.begin().await.expect("a transaction");
    let known = colliding.known(&mut tx).await.expect("the known rows read");
    tx.commit().await.expect("the read commits");
    let landed = known
        .iter()
        .find(|e| e.id == second)
        .and_then(|e| e.badge.clone())
        .expect("the second entity was badged");
    assert_ne!(
        landed, taken,
        "a collision must never leave two rows wearing the same badge"
    );

    store.stop().await;
}

/// **The other half of the same path: giving up.** A draw that always
/// collides must not hang and must not silently mint a duplicate — it
/// exhausts `ATTEMPTS` and the write comes back an error, the same as
/// `draw_free`'s own doc says `Ok(None)` means.
#[tokio::test]
async fn a_badge_draw_that_never_frees_gives_up_rather_than_duplicating() {
    let scratch = Scratch::new("badge-collision-giveup");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "badgecollisiongiveup")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");

    let memory = DoltMemory::open(pool.clone());
    let first = EntityId::person("person:contract-badge-collision-blocker");
    memory
        .add_entity(NewEntity::new(first.clone(), "First", "contract-fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the guard waves it through");

    let mut tx = pool.begin().await.expect("a transaction");
    let known = memory.known(&mut tx).await.expect("the known rows read");
    tx.commit().await.expect("the read commits");
    let taken = known
        .iter()
        .find(|e| e.id == first)
        .and_then(|e| e.badge.clone())
        .expect("the first entity was badged");

    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let never_frees: Draw = {
        let calls = calls.clone();
        let taken = taken.clone();
        std::sync::Arc::new(move || {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            taken.clone()
        })
    };

    let stuck = DoltMemory::open_drawing(pool.clone(), never_frees);
    let second = EntityId::person("person:contract-badge-collision-stuck");
    let result = stuck
        .add_entity(NewEntity::new(second.clone(), "Second", "contract-fixture"))
        .await;
    assert!(
        result.is_err(),
        "a draw that never finds a free badge must fail the write, not hang or duplicate: \
             {result:?}"
    );
    assert!(
        calls.load(std::sync::atomic::Ordering::SeqCst) >= 64,
        "giving up must mean every attempt was spent, not a premature bailout"
    );

    store.stop().await;
}

/// **`current_handle` must answer exactly what `entity_wearing` over the
/// full `known` list would answer, for every badge a real corpus holds.**
///
/// The contract's own run leaves behind entities, renames and former
/// handles — everything `current_handle` exists to resolve. Comparing its
/// answer against the list-search it replaces, badge by badge, whole
/// value against whole value, is what catches a fast path that gets the
/// common row right and a collision or a never-badged handle wrong.
#[tokio::test]
async fn current_handle_agrees_with_entity_wearing_for_every_badge_the_real_store_holds() {
    let scratch = Scratch::new("current-handle");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "currenthandle")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");

    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::testing::contract::run_all(&memory).await;

    let mut tx = pool.begin().await.expect("a transaction");
    let known = memory.known(&mut tx).await.expect("the known rows read");
    assert!(
        known.len() > 50,
        "the contract should have left far more than {} entities behind",
        known.len()
    );

    let mut checked = 0;
    for entity in &known {
        let mut candidates = vec![entity.id.clone()];
        if let Some(badge) = &entity.badge {
            candidates.push(EntityId(badge.clone()));
        }
        for candidate in candidates {
            let expected = jojobot_domain::memory::entity_wearing(candidate.as_str(), &known)
                .map(|e| e.id.clone())
                .unwrap_or_else(|| candidate.clone());
            let actual = memory
                .current_handle(&mut tx, &candidate)
                .await
                .expect("current_handle answers");
            assert_eq!(
                actual, expected,
                "current_handle disagreed with entity_wearing over {candidate}",
            );
            checked += 1;
        }
    }
    assert!(
        checked > 50,
        "too few candidates were actually checked ({checked})",
    );

    let missing = EntityId("person:contract-nobody".into());
    assert_eq!(
        memory
            .current_handle(&mut tx, &missing)
            .await
            .expect("current_handle answers"),
        missing,
        "a value nobody wears comes back unchanged",
    );
    tx.commit().await.expect("the read commits");

    // **A deliberate rename**, not one incidentally left behind by the
    // contract: the badge does not move when the handle does, so this is
    // the one case that actually exercises "resolves to the handle it
    // wears NOW" rather than "resolves to whichever handle it always
    // wore."
    let was = EntityId::person("person:contract-current-handle-was");
    let now = EntityId::person("person:contract-current-handle-now");
    memory
        .add_entity(NewEntity::new(
            was.clone(),
            "Current Handle Rename Subject",
            "contract-fixture",
        ))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the guard waves it through");
    memory
        .rename_entity(&was, &now, None, date(2026, 8, 10), None, None)
        .await
        .expect("rename_entity ok")
        .written()
        .expect("the guard waves it through");

    let mut tx = pool.begin().await.expect("a transaction");
    let known = memory.known(&mut tx).await.expect("the known rows read");
    let badge = known
        .iter()
        .find(|e| e.id == now)
        .and_then(|e| e.badge.clone())
        .expect("the renamed entity kept its badge");
    assert_eq!(
        jojobot_domain::memory::entity_wearing(&badge, &known).map(|e| e.id.clone()),
        Some(now.clone()),
        "the fixture itself should resolve the badge to the new handle",
    );
    assert_eq!(
        memory
            .current_handle(&mut tx, &EntityId(badge))
            .await
            .expect("current_handle answers"),
        now,
        "current_handle did not follow a rename to the handle its badge wears now",
    );
    tx.commit().await.expect("the read commits");

    store.stop().await;
}

/// A role's exclusivity against the real store — the same suite the
/// fake answers for, run against a disposable Dolt instance this test
/// spawns and tears down itself.
#[tokio::test]
async fn dolt_satisfies_the_role_claim_contract() {
    let scratch = Scratch::new("role-claims");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "roleclaims")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");

    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::testing::contract::run_all_role_claims(&memory).await;

    store.stop().await;
}

/// 🚨 **A value held by a write nobody stamped has no age, and the stores say
/// so the same way.** Writes appended before `field_write.written_at` existed
/// carry NULL there and always will: dating them would claim jojobot learned
/// something at a moment nobody observed. The key arrives without an age while
/// the thing's newest moment is still known from its stamped claim writes, and
/// a later stamped rewrite of the key gives it one.
#[tokio::test]
async fn a_key_held_by_an_unstamped_write_has_no_age() {
    let scratch = Scratch::new("unstamped-key");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("unstamped")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");

    let subject = EntityId::person("person:milhouse");
    memory
        .add_entity(NewEntity::new(
            subject.clone(),
            "Milhouse",
            "contract-fixture",
        ))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the guard waves it through");
    let write = |value: &str| {
        jojobot_domain::memory::testing::contract::state(NewFact {
            fields: [("colour".to_string(), value.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(subject.clone(), "painted", date(2026, 10, 7))
        })
    };
    memory
        .capture(write("red"))
        .await
        .expect("capture ok")
        .written()
        .expect("the guard waves it through");
    let stamped = memory.field_ages(&subject).await.expect("ages read");
    assert!(
        stamped.keys.contains_key("colour"),
        "the positive: a stamped write gives the key an age: {stamped:?}"
    );

    sqlx::query("UPDATE field_write SET written_at = NULL")
        .execute(&pool)
        .await
        .expect("the stamps are taken off");
    let unstamped = memory.field_ages(&subject).await.expect("ages read");
    assert!(
        !unstamped.keys.contains_key("colour"),
        "a write nobody stamped was given an age: {unstamped:?}"
    );
    assert!(
        unstamped.newest.is_some(),
        "the claim write beside it is still stamped: {unstamped:?}"
    );

    memory
        .capture(write("blue"))
        .await
        .expect("capture ok")
        .written()
        .expect("the guard waves it through");
    let again = memory.field_ages(&subject).await.expect("ages read");
    assert!(
        again.keys.contains_key("colour"),
        "a later stamped write gives the key an age again: {again:?}"
    );

    store.stop().await;
}

/// One write of `status` as a merge sees it: its place in its own thing's
/// history and the moment the store stamped it (`None` for a write an older
/// build appended).
fn written(ordinal: i64, at: Option<&str>) -> StampedWrite {
    StampedWrite {
        key: "status".to_string(),
        ordinal,
        fact_id: "f1".to_string(),
        written_at: at.map(|text| text.parse().expect("a stamp")),
    }
}

/// What order a merge puts two histories in, as (side, original ordinal),
/// oldest first.
fn order(folded: &[StampedWrite], survivor: &[StampedWrite]) -> Vec<(Side, i64)> {
    interleave(folded, survivor)
        .into_iter()
        .map(|(side, write)| (side, write.ordinal))
        .collect()
}

/// **The write made later is placed later, whichever thing held it.** Both
/// directions are asserted, because an order that always put the survivor's
/// writes first passes one of them by luck.
#[test]
fn the_later_write_is_placed_later_whichever_thing_held_it() {
    let early = written(1, Some("2026-08-02T10:00:00Z"));
    let late = written(1, Some("2026-08-03T10:00:00Z"));
    assert_eq!(
        order(std::slice::from_ref(&early), std::slice::from_ref(&late)),
        vec![(Side::Folded, 1), (Side::Survivor, 1)],
        "the survivor's write is the later one and is placed last",
    );
    assert_eq!(
        order(&[late], &[early]),
        vec![(Side::Survivor, 1), (Side::Folded, 1)],
        "the duplicate's write is the later one and is placed last",
    );
}

/// **On a tie the survivor's write is placed after the duplicate's**, so the
/// survivor wins, and the same holds when neither write was ever stamped.
#[test]
fn a_tie_places_the_survivors_write_last() {
    let same = written(1, Some("2026-08-03T10:00:00Z"));
    assert_eq!(
        order(std::slice::from_ref(&same), std::slice::from_ref(&same)),
        vec![(Side::Folded, 1), (Side::Survivor, 1)],
    );
    let legacy = written(1, None);
    assert_eq!(
        order(std::slice::from_ref(&legacy), std::slice::from_ref(&legacy)),
        vec![(Side::Folded, 1), (Side::Survivor, 1)],
        "two writes with no stamp are a tie too",
    );
}

/// **A write with no stamp is older than every stamped one.** The stamp came
/// into being at one moment, so every write that carries one is later than every
/// write that cannot.
#[test]
fn an_unstamped_write_is_older_than_any_stamped_one() {
    let legacy = written(1, None);
    let stamped = written(1, Some("2026-08-02T10:00:00Z"));
    assert_eq!(
        order(
            std::slice::from_ref(&legacy),
            std::slice::from_ref(&stamped)
        ),
        vec![(Side::Folded, 1), (Side::Survivor, 1)],
    );
    assert_eq!(
        order(&[stamped], &[legacy]),
        vec![(Side::Survivor, 1), (Side::Folded, 1)],
        "the survivor's legacy write is older than the duplicate's stamped one",
    );
}

/// **Each thing's own order survives the merge**, even when a clock stepped
/// back inside one history: the two are merged, not sorted, so no write passes
/// another write of its own thing.
#[test]
fn a_things_own_order_is_kept_when_its_clock_stepped_back() {
    let first = written(1, Some("2026-08-05T10:00:00Z"));
    let second = written(2, Some("2026-08-01T10:00:00Z"));
    let other = written(1, Some("2026-08-03T10:00:00Z"));
    let merged = order(&[first, second], &[other]);
    let own: Vec<i64> = merged
        .iter()
        .filter(|(side, _)| *side == Side::Folded)
        .map(|(_, ordinal)| *ordinal)
        .collect();
    assert_eq!(own, vec![1, 2], "the duplicate's writes were reordered");
    assert_eq!(merged.len(), 3, "every write is placed: {merged:?}");
}

/// **Two things merged, and the two shapes the store can hold the merge in.**
/// The merge moves the folded thing's claims onto the survivor today. A merge
/// that leaves them where they were made is the same merge with its rows put
/// back, which is what [`Merged::leave_in_place`] does to the store, with the
/// event untouched. Every read family holds the two shapes to one answer.
struct Merged {
    memory: DoltMemory,
    pool: sqlx::MySqlPool,
    survivor: EntityId,
    folded: EntityId,
    survivor_key: EntityId,
    folded_key: EntityId,
    /// A third thing, and its one claim that points at the folded thing and at
    /// the folded thing's first claim.
    pointing: EntityId,
    pointing_key: EntityId,
    pointing_claim: FactAddress,
    /// The own claim of the survivor.
    own: FactAddress,
    /// For each claim the merge moved: the id it had and the id it was given.
    moved: Vec<(String, String)>,
}

impl Merged {
    /// Make the two things, give each claims, merge them, and keep the keys.
    /// The folded thing's second claim is written twice. **The rows are then
    /// moved as the merge before this one moved them**, so the store is the one a
    /// deployed build filled; [`Merged::before`] and [`Merged::fold`] alone leave
    /// the store as the merge does now.
    async fn make(pool: sqlx::MySqlPool) -> Merged {
        let mut merged = Merged::before(pool).await;
        merged.fold().await;
        merged.move_rows_as_the_older_merge_did().await;
        merged
    }

    /// Make the two things and give each claims, and keep the keys. Nothing is
    /// merged yet, and `moved` is empty until [`Merged::fold`] lands the merge.
    async fn before(pool: sqlx::MySqlPool) -> Merged {
        let memory = DoltMemory::open(pool.clone());
        jojobot_domain::memory::kinds::seed(&memory)
            .await
            .expect("the kinds are seeded");
        let survivor = EntityId::person("person:homer");
        let folded = EntityId::person("person:apu");
        let pointing = EntityId("bot:moe".into());
        for (handle, name) in [(&survivor, "Homer"), (&folded, "Apu"), (&pointing, "Moe")] {
            let mut entity = NewEntity::new(handle.clone(), name, "fixture");
            if handle == &folded {
                entity.aliases = vec!["Squishee Man".into()];
            }
            memory
                .add_entity(entity)
                .await
                .expect("add_entity ok")
                .written()
                .expect("the guard waves it through");
        }
        // A thing under the folded one, which the merge re-points at the survivor.
        let mut child = NewEntity::new(EntityId("place:leftorium".into()), "Leftorium", "fixture");
        child.parent = Some(folded.clone());
        memory
            .add_entity(child)
            .await
            .expect("add_entity ok")
            .written()
            .expect("the guard waves it through");
        // Fields written in an order that tells a merge that put the folded
        // thing's writes first, or last, from one that places them by the moment
        // each was made: `mood` is written by the survivor, the folded thing and
        // the survivor again; `lunch` by the survivor and then the folded thing.
        let said = |about: &EntityId, what: &str, fields: &[(&str, &str)]| {
            let mut fact = NewFact::about(about.clone(), what, date(2026, 9, 1));
            fact.fields = fields
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect();
            // A key named `own_...` stays on the record; every other key is set
            // on the thing.
            fact.sets = fact
                .fields
                .keys()
                .filter(|key| !key.starts_with("own_"))
                .cloned()
                .collect();
            fact
        };
        let own = memory
            .capture(said(
                &survivor,
                "keeps bees",
                &[("city", "Springfield"), ("mood", "calm"), ("lunch", "soup")],
            ))
            .await
            .expect("capture ok")
            .written()
            .expect("the guard waves it through")
            .address();
        let mut first_claim = said(
            &folded,
            "runs the store",
            &[
                ("mood", "angry"),
                ("job", "clerk"),
                ("lunch", "salad"),
                ("reports_to", "bot:moe"),
            ],
        );
        // A reference of the claim's own, which moves with the claim.
        first_claim.refs = vec![pointing.clone()];
        let first = memory
            .capture(first_claim)
            .await
            .expect("capture ok")
            .written()
            .expect("the guard waves it through")
            .address();
        let second = memory
            .capture(said(&folded, "closes at night", &[("own_shift", "night")]))
            .await
            .expect("capture ok")
            .written()
            .expect("the guard waves it through")
            .address();
        memory
            .capture(said(&survivor, "keeps a car", &[("mood", "cheerful")]))
            .await
            .expect("capture ok")
            .written()
            .expect("the guard waves it through");
        memory
            .update_fact(
                &second,
                FactPatch {
                    content: Some("closes at midnight".into()),
                    provenance: Some(Provenance::Inference),
                    // A mark of the claim's own, which moves with the claim.
                    stands_for: Some(vec![own.clone()]),
                    ..Default::default()
                },
                &EntityId("bot:sigma".into()),
            )
            .await
            .expect("update_fact ok")
            .written()
            .expect("the guard waves it through");
        // A third thing's claim that points at the folded thing in every way a
        // claim can: an edge, a reference, a field naming it, the claim it was
        // derived from, and a mark naming that claim.
        // Two lists that name both sides of the merge, one under a key nothing
        // declares and one under a declared reference list.
        memory
            .declare_type(
                jojobot_domain::memory::types::DeclaredType::new(
                    "regulars-fixture",
                    vec![jojobot_domain::memory::types::Field::listing(
                        "regulars",
                        jojobot_domain::memory::types::ValueType::Reference,
                    )],
                ),
                None,
            )
            .await
            .expect("the type is declared");
        let mut pointer = said(
            &pointing,
            "knows the grocer",
            &[
                ("friend", "person:apu"),
                ("friends", "person:apu, person:homer"),
                ("regulars", "person:apu, person:homer"),
            ],
        );
        pointer.edge = Some(Edge::new(EdgeShape::Connection, folded.clone()));
        pointer.refs = vec![folded.clone()];
        pointer.derived_from = Some(first.clone());
        let pointing_claim = memory
            .capture(pointer)
            .await
            .expect("capture ok")
            .written()
            .expect("the guard waves it through")
            .address();
        memory
            .update_fact(
                &pointing_claim,
                FactPatch {
                    stands_for: Some(vec![first.clone()]),
                    ..Default::default()
                },
                &EntityId("bot:sigma".into()),
            )
            .await
            .expect("update_fact ok")
            .written()
            .expect("the guard waves it through");
        let mut tx = pool.begin().await.expect("a transaction");
        let (survivor_key, _) = memory
            .resolve(&mut tx, &survivor)
            .await
            .expect("resolve ok")
            .expect("the survivor exists");
        let (folded_key, _) = memory
            .resolve(&mut tx, &folded)
            .await
            .expect("resolve ok")
            .expect("the folded thing exists");
        let (pointing_key, _) = memory
            .resolve(&mut tx, &pointing)
            .await
            .expect("resolve ok")
            .expect("the pointing thing exists");
        tx.commit().await.expect("the read commits");
        Merged {
            memory,
            pool,
            survivor,
            folded,
            survivor_key,
            folded_key,
            pointing,
            pointing_key,
            pointing_claim,
            own,
            moved: Vec::new(),
        }
    }

    /// [`move_rows_as_the_older_merge_did`] for this pair.
    async fn move_rows_as_the_older_merge_did(&self) {
        move_rows_as_the_older_merge_did(
            &self.pool,
            &self.folded_key,
            &self.survivor_key,
            &self.moved,
        )
        .await;
    }

    /// Fold the one thing into the other, and keep the map the merge event holds.
    async fn fold(&mut self) {
        self.memory
            .merge(
                &self.folded,
                &self.survivor,
                Some("filed twice"),
                date(2026, 9, 2),
                // The manager the folded thing reports to is the one who may
                // fold it into another thing.
                &self.pointing,
                Some("run-a"),
            )
            .await
            .expect("the fold lands");
        let event = self
            .memory
            .merge_events(Some(&self.survivor))
            .await
            .expect("events read")
            .pop()
            .expect("the merge left an event");
        self.moved = event
            .claims
            .iter()
            .map(|(was, now)| (was.as_str().to_string(), now.as_str().to_string()))
            .collect();
    }

    /// Put the folded thing's claims back under the key and the ids they were
    /// made under, leaving the event and the survivor's own claims as they are.
    async fn leave_in_place(&self) {
        self.leave_names_in_place().await;
        self.leave_pointers_in_place().await;
        put_field_writes_back(
            &self.pool,
            &self.survivor_key,
            &self.folded_key,
            &self.moved,
        )
        .await;
        put_claims_back(
            &self.pool,
            &self.survivor_key,
            &self.folded_key,
            &self.moved,
        )
        .await;
    }

    /// Put the child back under the folded thing, and the folded thing's own
    /// alias back on it, as they were before the merge moved them. The merge
    /// carried its display name as an alias too, which it never had.
    async fn leave_names_in_place(&self) {
        sqlx::query("UPDATE entity SET parent = ? WHERE id = ?")
            .bind(self.folded_key.as_str())
            .bind("place:leftorium")
            .execute(&self.pool)
            .await
            .expect("the child goes back");
        sqlx::query("UPDATE entity_alias SET entity = ? WHERE entity = ? AND alias = ?")
            .bind(self.folded_key.as_str())
            .bind(self.survivor_key.as_str())
            .bind("Squishee Man")
            .execute(&self.pool)
            .await
            .expect("the alias goes back");
        sqlx::query("DELETE FROM entity_alias WHERE entity = ? AND alias = ?")
            .bind(self.survivor_key.as_str())
            .bind("Apu")
            .execute(&self.pool)
            .await
            .expect("the carried display name goes");
    }

    /// Put the pointers of the third thing's claim back at the folded thing and
    /// its claims, as they were before the merge rewrote them.
    async fn leave_pointers_in_place(&self) {
        for table in ["fact", "fact_write"] {
            sqlx::query(&format!(
                "UPDATE {table} SET edge_object = ? WHERE entity = ? AND edge_object = ?"
            ))
            .bind(self.folded_key.as_str())
            .bind(self.pointing_key.as_str())
            .bind(self.survivor_key.as_str())
            .execute(&self.pool)
            .await
            .expect("the edge goes back");
            for (was, now) in &self.moved {
                sqlx::query(&format!(
                    "UPDATE {table} SET derived_from = ?, derived_from_id = ?
                     WHERE entity = ? AND derived_from = ? AND derived_from_id = ?"
                ))
                .bind(self.folded_key.as_str())
                .bind(was)
                .bind(self.pointing_key.as_str())
                .bind(self.survivor_key.as_str())
                .bind(now)
                .execute(&self.pool)
                .await
                .expect("the lineage goes back");
            }
        }
        sqlx::query("UPDATE fact_event_ref SET entity = ? WHERE fact_home = ? AND entity = ?")
            .bind(self.folded_key.as_str())
            .bind(self.pointing_key.as_str())
            .bind(self.survivor_key.as_str())
            .execute(&self.pool)
            .await
            .expect("the reference goes back");
        for (was, now) in &self.moved {
            sqlx::query(
                "UPDATE fact_stands_for SET source_home = ?, source_id = ?
                 WHERE fact_home = ? AND source_home = ? AND source_id = ?",
            )
            .bind(self.folded_key.as_str())
            .bind(was)
            .bind(self.pointing_key.as_str())
            .bind(self.survivor_key.as_str())
            .bind(now)
            .execute(&self.pool)
            .await
            .expect("the mark goes back");
        }
        let mut tx = self.pool.begin().await.expect("a transaction");
        let declared = DoltMemory::types_in(&mut tx).await.expect("types read");
        DoltMemory::repoint_field_links(
            &mut tx,
            &declared,
            self.survivor_key.as_str(),
            self.folded_key.as_str(),
        )
        .await
        .expect("the field goes back");
        tx.commit().await.expect("the pointers commit");
        // A list that named both sides was cut to one item by the merge, and
        // putting the link back cannot bring the other item back: each list is
        // written as it stood, naming the folded thing and the survivor. An
        // undeclared list keeps the mark on its ids; a declared one holds them
        // bare.
        let marked = |id: &EntityId| jojobot_domain::memory::marked_stored_handle(id.as_str());
        for (key, both) in [
            (
                "friends",
                format!(
                    "{}, {}",
                    marked(&self.folded_key),
                    marked(&self.survivor_key)
                ),
            ),
            (
                "regulars",
                format!(
                    "{}, {}",
                    self.folded_key.as_str(),
                    self.survivor_key.as_str()
                ),
            ),
        ] {
            let cut =
                sqlx::query("UPDATE field_write SET value = ? WHERE entity = ? AND `key` = ?")
                    .bind(&both)
                    .bind(self.pointing_key.as_str())
                    .bind(key)
                    .execute(&self.pool)
                    .await
                    .expect("the list goes back");
            assert_eq!(cut.rows_affected(), 1, "one write of {key}");
            sqlx::query(
                "INSERT IGNORE INTO field_link (entity, `key`, ordinal, target)
                 SELECT entity, `key`, ordinal, ? FROM field_write
                 WHERE entity = ? AND `key` = ?",
            )
            .bind(self.folded_key.as_str())
            .bind(self.pointing_key.as_str())
            .bind(key)
            .execute(&self.pool)
            .await
            .expect("the link goes back");
        }
    }
}

/// Put the folded thing's field writes back under its key and its claims'
/// old ids, and number each side's writes of a key from one in the order
/// they stand, as they were before the merge interleaved them.
async fn put_field_writes_back(
    pool: &sqlx::MySqlPool,
    survivor_key: &EntityId,
    folded_key: &EntityId,
    moved: &[(String, String)],
) {
    let came_from_folded: std::collections::HashMap<&str, &str> = moved
        .iter()
        .map(|(was, now)| (now.as_str(), was.as_str()))
        .collect();
    let rows = sqlx::query(
        "SELECT `key`, ordinal, fact_id FROM field_write
         WHERE entity = ? ORDER BY `key`, ordinal",
    )
    .bind(survivor_key.as_str())
    .fetch_all(pool)
    .await
    .expect("the writes read");
    let mut placed: std::collections::HashMap<(String, bool), i64> = Default::default();
    for row in &rows {
        let key: String = row.get("key");
        let ordinal: i64 = row.get("ordinal");
        let fact: String = row.get("fact_id");
        let folded = came_from_folded.get(fact.as_str());
        let next = placed.entry((key.clone(), folded.is_some())).or_insert(0);
        *next += 1;
        sqlx::query(
            "UPDATE field_write SET entity = ?, fact_id = ?, ordinal = ?
             WHERE entity = ? AND `key` = ? AND ordinal = ?",
        )
        .bind(if folded.is_some() {
            folded_key.as_str()
        } else {
            survivor_key.as_str()
        })
        .bind(folded.copied().unwrap_or(fact.as_str()))
        .bind(*next + PARKED)
        .bind(survivor_key.as_str())
        .bind(&key)
        .bind(ordinal)
        .execute(pool)
        .await
        .expect("the write goes back");
    }
    sqlx::query("UPDATE field_write SET ordinal = ordinal - ? WHERE ordinal >= ?")
        .bind(PARKED)
        .bind(PARKED)
        .execute(pool)
        .await
        .expect("the writes come down");
}

/// Put the claim rows a merge moved back under the folded key and the ids they
/// had, leaving the event as it is.
async fn put_claims_back(
    pool: &sqlx::MySqlPool,
    survivor_key: &EntityId,
    folded_key: &EntityId,
    moved: &[(String, String)],
) {
    for (was, now) in moved {
        for (table, id_column) in [("fact", "id"), ("fact_write", "fact_id")] {
            let moved_back = sqlx::query(&format!(
                "UPDATE {table} SET entity = ?, {id_column} = ? WHERE entity = ? AND {id_column} = ?"
            ))
            .bind(folded_key.as_str())
            .bind(was)
            .bind(survivor_key.as_str())
            .bind(now)
            .execute(pool)
            .await
            .expect("the rows go back");
            assert!(
                moved_back.rows_affected() > 0,
                "{table}: no row of {now} to put back, so the case would read the moved shape"
            );
        }
        // The references and marks that belong to the claim move with it.
        for table in ["fact_event_metadata", "fact_event_ref", "fact_stands_for"] {
            sqlx::query(&format!(
                "UPDATE {table} SET fact_home = ?, fact_id = ? WHERE fact_home = ? AND fact_id = ?"
            ))
            .bind(folded_key.as_str())
            .bind(was)
            .bind(survivor_key.as_str())
            .bind(now)
            .execute(pool)
            .await
            .expect("the claim's own rows go back");
        }
    }
}

/// **What the merge of the code before this one did to the rows**, kept so a case
/// can build a store a deployed build filled. The merge no longer moves a row, so
/// the shape of an older merge is made by applying these moves to the store the
/// merge left.
impl DoltMemory {
    /// **Move the duplicate's field writes onto the survivor, in the order they
    /// were made.**
    ///
    /// The fold of a thing's fields takes the newest write of each key by
    /// ordinal, so a merge that put the duplicate's writes after the survivor's
    /// would make the duplicate's value win whatever its age. Each key's two
    /// histories are interleaved by the moment the store stamped each write
    /// ([`interleave`]) and both sides are renumbered from one as one mapping,
    /// across `field_write` and `field_link`, which is keyed by the write's
    /// address. A claim's new id comes from `renamed`.
    ///
    /// **The move goes through a parked range.** The primary key is (thing, key,
    /// ordinal), so renumbering in place would collide half way. Every row is
    /// first set to its final ordinal plus [`PARKED`], a range nothing holds,
    /// then the whole range is brought down in one statement.
    async fn interleave_writes(
        tx: &mut Transaction<'_, MySql>,
        folded: &str,
        survivor: &str,
        renamed: &std::collections::HashMap<String, String>,
    ) -> Result<(), MemoryError> {
        let mut by_key: std::collections::BTreeMap<String, (Vec<StampedWrite>, Vec<StampedWrite>)> =
            std::collections::BTreeMap::new();
        for write in Self::stamped_writes(tx, folded).await? {
            by_key.entry(write.key.clone()).or_default().0.push(write);
        }
        for write in Self::stamped_writes(tx, survivor).await? {
            by_key.entry(write.key.clone()).or_default().1.push(write);
        }
        for (key, (theirs, ours)) in by_key {
            // A key only the survivor holds has nothing to interleave.
            if theirs.is_empty() {
                continue;
            }
            for (place, (side, write)) in interleave(&theirs, &ours).into_iter().enumerate() {
                let (from, fact_id) = match side {
                    Side::Folded => (
                        folded,
                        renamed.get(&write.fact_id).unwrap_or(&write.fact_id),
                    ),
                    Side::Survivor => (survivor, &write.fact_id),
                };
                let parked = place as i64 + 1 + PARKED;
                sqlx::query(
                    "UPDATE field_write SET entity = ?, fact_id = ?, ordinal = ? \
                     WHERE entity = ? AND `key` = ? AND ordinal = ?",
                )
                .bind(survivor)
                .bind(fact_id)
                .bind(parked)
                .bind(from)
                .bind(&key)
                .bind(write.ordinal)
                .execute(&mut **tx)
                .await
                .map_err(store)?;
                sqlx::query(
                    "UPDATE field_link SET entity = ?, ordinal = ? \
                     WHERE entity = ? AND `key` = ? AND ordinal = ?",
                )
                .bind(survivor)
                .bind(parked)
                .bind(from)
                .bind(&key)
                .bind(write.ordinal)
                .execute(&mut **tx)
                .await
                .map_err(store)?;
            }
        }
        for table in ["field_write", "field_link"] {
            sqlx::query(&format!(
                "UPDATE {table} SET ordinal = ordinal - ? WHERE entity = ? AND ordinal >= ?"
            ))
            .bind(PARKED)
            .bind(survivor)
            .bind(PARKED)
            .execute(&mut **tx)
            .await
            .map_err(store)?;
        }
        Ok(())
    }

    /// Every field write on one thing with the moment the store stamped it,
    /// each key's writes in ordinal order. **A stamp that does not parse reads as
    /// no stamp**, the same as a write that was never stamped.
    async fn stamped_writes(
        tx: &mut Transaction<'_, MySql>,
        entity: &str,
    ) -> Result<Vec<StampedWrite>, MemoryError> {
        let rows = sqlx::query(
            "SELECT `key`, ordinal, fact_id, written_at FROM field_write \
             WHERE entity = ? ORDER BY `key`, ordinal",
        )
        .bind(entity)
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let mut writes = Vec::with_capacity(rows.len());
        for row in &rows {
            let key: String = row.try_get("key").map_err(store)?;
            let stamp: Option<String> = row.try_get("written_at").map_err(store)?;
            let written_at = stamp.and_then(|text| match text.parse::<jiff::Timestamp>() {
                Ok(at) => Some(at),
                Err(e) => {
                    tracing::warn!(error = %e, entity, key, "a write's stamp does not parse; it reads as unstamped");
                    None
                }
            });
            writes.push(StampedWrite {
                ordinal: row.try_get("ordinal").map_err(store)?,
                fact_id: row.try_get("fact_id").map_err(store)?,
                key,
                written_at,
            });
        }
        Ok(writes)
    }
}

/// The first ordinal of the range a merge parks field writes in while it
/// renumbers them: far above any count of writes of one key.
const PARKED: i64 = 1 << 40;

/// **Move the folded thing's rows onto the survivor as the merge before this
/// one did**, from the store the merge leaves: every claim under the id the
/// event gave it, its writes, its references and marks, the field writes
/// interleaved by moment, every pointer at the folded thing, and its child's
/// parent. The event is not touched.
async fn move_rows_as_the_older_merge_did(
    pool: &sqlx::MySqlPool,
    folded_key: &EntityId,
    survivor_key: &EntityId,
    moved: &[(String, String)],
) {
    let renamed: std::collections::HashMap<String, String> = moved.iter().cloned().collect();
    let mut tx = pool.begin().await.expect("a transaction");
    for (was, now) in moved {
        for statement in [
            "UPDATE fact SET entity = ?, id = ? WHERE entity = ? AND id = ?",
            "UPDATE fact SET derived_from = ?, derived_from_id = ? \
             WHERE derived_from = ? AND derived_from_id = ?",
            "UPDATE fact_write SET entity = ?, fact_id = ? WHERE entity = ? AND fact_id = ?",
            "UPDATE fact_write SET derived_from = ?, derived_from_id = ? \
             WHERE derived_from = ? AND derived_from_id = ?",
            "UPDATE fact_event_metadata SET fact_home = ?, fact_id = ? \
             WHERE fact_home = ? AND fact_id = ?",
            "UPDATE fact_event_ref SET fact_home = ?, fact_id = ? \
             WHERE fact_home = ? AND fact_id = ?",
            "UPDATE fact_stands_for SET fact_home = ?, fact_id = ? \
             WHERE fact_home = ? AND fact_id = ?",
            "UPDATE fact_stands_for SET source_home = ?, source_id = ? \
             WHERE source_home = ? AND source_id = ?",
        ] {
            sqlx::query(statement)
                .bind(survivor_key.as_str())
                .bind(now)
                .bind(folded_key.as_str())
                .bind(was)
                .execute(&mut *tx)
                .await
                .expect("the row moves");
        }
    }
    DoltMemory::interleave_writes(
        &mut tx,
        folded_key.as_str(),
        survivor_key.as_str(),
        &renamed,
    )
    .await
    .expect("the writes interleave");
    for statement in [
        "UPDATE fact SET edge_object = ? WHERE edge_object = ?",
        "UPDATE fact_write SET edge_object = ? WHERE edge_object = ?",
        "UPDATE fact_event_ref SET entity = ? WHERE entity = ?",
        "UPDATE entity SET parent = ? WHERE parent = ?",
    ] {
        sqlx::query(statement)
            .bind(survivor_key.as_str())
            .bind(folded_key.as_str())
            .execute(&mut *tx)
            .await
            .expect("the pointer moves");
    }
    let declared = DoltMemory::types_in(&mut tx).await.expect("types read");
    DoltMemory::repoint_field_links(
        &mut tx,
        &declared,
        folded_key.as_str(),
        survivor_key.as_str(),
    )
    .await
    .expect("the links move");
    tx.commit().await.expect("the move commits");
}

/// Every cell of every row a query returns, as text: a row is compared whole
/// whatever its columns hold.
async fn dump(pool: &sqlx::MySqlPool, sql: &str, binds: &[&str]) -> Vec<Vec<Option<String>>> {
    use sqlx::Row as _;
    let mut query = sqlx::query(sql);
    for bind in binds {
        query = query.bind(*bind);
    }
    let mut rows: Vec<Vec<Option<String>>> = query
        .fetch_all(pool)
        .await
        .expect("the rows read")
        .iter()
        .map(|row| {
            (0..row.columns().len())
                .map(|at| {
                    row.try_get::<Option<String>, _>(at)
                        .or_else(|_| {
                            row.try_get::<Option<i64>, _>(at)
                                .map(|n| n.map(|n| n.to_string()))
                        })
                        .expect("a cell reads as text or a number")
                })
                .collect()
        })
        .collect();
    rows.sort();
    rows
}

impl Merged {
    /// **The stored rows a merge used to rewrite**: the folded thing's own, the
    /// third thing's claim that points at it in every way a claim can, and the
    /// child that named it as its parent.
    async fn rows_a_merge_used_to_rewrite(&self) -> Vec<(String, Vec<Vec<Option<String>>>)> {
        let folded = self.folded_key.as_str();
        let pointing = self.pointing_key.as_str();
        let mut out = Vec::new();
        for (table, column) in [
            ("fact", "entity"),
            ("fact_write", "entity"),
            ("field_write", "entity"),
            ("field_link", "entity"),
            ("fact_event_metadata", "fact_home"),
            ("fact_event_ref", "fact_home"),
            ("fact_stands_for", "fact_home"),
        ] {
            for (whose, key) in [("folded", folded), ("pointing", pointing)] {
                out.push((
                    format!("{table} of the {whose} thing"),
                    dump(
                        &self.pool,
                        &format!("SELECT * FROM {table} WHERE {column} = ?"),
                        &[key],
                    )
                    .await,
                ));
            }
        }
        out.push((
            "the child".into(),
            dump(
                &self.pool,
                "SELECT id, parent FROM entity WHERE id = ?",
                &["place:leftorium"],
            )
            .await,
        ));
        out
    }
}

/// **A merge writes its event and nothing else of the folded thing's**: the
/// rows that held its claims, their writes, its field writes and links, the
/// pointers other claims hold at it and its child's parent all stay as they were
/// stored, and the survivor still serves every one of them.
#[tokio::test]
async fn a_merge_leaves_the_stored_rows_of_the_folded_thing_as_they_were() {
    let scratch = Scratch::new("merge-leaves-rows");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "merge_leaves_rows")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let mut merged = Merged::before(pool.clone()).await;

    let before = merged.rows_a_merge_used_to_rewrite().await;
    assert!(
        before.iter().any(|(_, rows)| !rows.is_empty()),
        "the case dumps rows, so an empty comparison cannot pass"
    );
    merged.fold().await;
    let after = merged.rows_a_merge_used_to_rewrite().await;
    for ((table, was), (_, now)) in before.iter().zip(&after) {
        assert_eq!(was, now, "{table} was rewritten by the merge");
    }

    // The positive half: the survivor serves the folded claims under the ids the
    // event gave them, and the merge event reads back with its run.
    let mut tx = pool.begin().await.expect("a transaction");
    let served = merged
        .memory
        .facts_projected(&mut tx, &merged.survivor_key)
        .await
        .expect("the survivor projects");
    tx.commit().await.expect("the read commits");
    assert_eq!(
        served.len(),
        5,
        "the survivor's own, two folded claims and the account: {served:?}"
    );
    for (_, now) in &merged.moved {
        assert!(
            served.iter().any(|fact| fact.id.as_str() == now),
            "{now} is served on the survivor"
        );
    }
    let ids: std::collections::BTreeSet<&str> =
        served.iter().map(|fact| fact.id.as_str()).collect();
    assert_eq!(
        ids.len(),
        served.len(),
        "no two claims are served under one id: {served:?}"
    );
    let event = merged
        .memory
        .merge_events(Some(&merged.survivor))
        .await
        .expect("events read")
        .pop()
        .expect("the merge left an event");
    assert_eq!(event.session.as_deref(), Some("run-a"));

    store.stop().await;
}

/// **A claim stored under the folded thing that no merge event lists is named,
/// not dropped.** The survivor serves every claim the event maps, names the one
/// it cannot serve as left out with the reason and the address that reads it,
/// and the address reads it. **Paired**: the mapped claims are all still served,
/// and a thing nobody merged into names nothing.
#[tokio::test]
async fn a_claim_under_the_folded_thing_that_no_event_lists_is_named_and_readable() {
    let scratch = Scratch::new("merge-unmapped");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "merge_unmapped")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let mut merged = Merged::before(pool.clone()).await;
    merged.fold().await;

    // A claim that landed under the folded key after the merge: the event holds
    // no id for it.
    for statement in [
        "INSERT INTO fact (entity, id, content, provenance, status, recorded_at)
         VALUES (?, 'f9', 'a stray claim', 'inference', 'active', '2026-09-03')",
        "INSERT INTO fact_write (entity, fact_id, ordinal, content, provenance, status, recorded_at)
         VALUES (?, 'f9', 1, 'a stray claim', 'inference', 'active', '2026-09-03')",
    ] {
        sqlx::query(statement)
            .bind(merged.folded_key.as_str())
            .execute(&pool)
            .await
            .expect("the stray claim lands");
    }
    let merge_no: i64 = sqlx::query_scalar("SELECT merge_no FROM merge_event WHERE ordinal = 0")
        .fetch_one(&pool)
        .await
        .expect("the event is there");

    let doc = merged
        .memory
        .scan_entity(&merged.survivor)
        .await
        .expect("the scan reads")
        .expect("the survivor is there");
    let address = FactAddress::new(merged.folded.clone(), FactId("f9".into()));
    assert_eq!(
        doc.unmapped,
        vec![jojobot_domain::memory::unmapped::UnmappedClaim {
            address: address.clone(),
            merge_no: merge_no as u64,
        }],
        "the claim is named"
    );
    assert_eq!(
        doc.unmapped[0].reason(),
        format!("unmapped by merge event {merge_no}")
    );
    // The positive half: everything the event maps is still served, and the
    // stray claim is not served under any id.
    assert_eq!(doc.facts.len(), 5, "{:?}", doc.facts);
    assert!(
        doc.facts.iter().all(|fact| fact.content != "a stray claim"),
        "the stray claim is left out of the survivor's claims"
    );
    // The address named reads the claim.
    let history = merged
        .memory
        .claim_history(&address)
        .await
        .expect("the address reads");
    assert_eq!(history.len(), 1, "{history:?}");
    // A thing nobody merged into names nothing.
    let other = merged
        .memory
        .scan_entity(&merged.pointing)
        .await
        .expect("the scan reads")
        .expect("the thing is there");
    assert!(other.unmapped.is_empty(), "{:?}", other.unmapped);
    store.stop().await;
}

/// **A merge whose claims were left in place reads, as to its claims, exactly
/// like a merge whose claims were moved** — the survivor lists them under its
/// own handle and the ids the event gave them, and the folded thing lists
/// nothing. The survivor's own claim reads as it did, so the case cannot pass
/// by rewriting every address.
#[tokio::test]
async fn claims_left_in_place_by_a_merge_read_like_claims_moved_by_it() {
    let scratch = Scratch::new("merged-claims");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "merged_claims")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let merged = Merged::make(pool.clone()).await;

    let read = |address: FactAddress| {
        let merged = &merged;
        async move {
            let mut tx = merged.pool.begin().await.expect("a transaction");
            let fact = merged
                .memory
                .fact_projected(&mut tx, &address)
                .await
                .expect("the substrate projects one");
            tx.commit().await.expect("the read commits");
            fact
        }
    };
    let list = |key: EntityId| {
        let merged = &merged;
        async move {
            let mut tx = merged.pool.begin().await.expect("a transaction");
            let facts = merged
                .memory
                .facts_projected(&mut tx, &key)
                .await
                .expect("the substrate projects");
            tx.commit().await.expect("the read commits");
            facts
        }
    };
    let served_as =
        |now: &str| FactAddress::new(merged.survivor_key.clone(), FactId(now.to_string()));

    // The moved shape is the control: what the reads answer today.
    let moved_list = list(merged.survivor_key.clone()).await;
    let mut moved_each = Vec::new();
    for (_, now) in &merged.moved {
        moved_each.push(read(served_as(now)).await);
    }
    let moved_own = read(FactAddress::new(
        merged.survivor_key.clone(),
        merged.own.local.clone(),
    ))
    .await;
    assert_eq!(
        moved_list.len(),
        5,
        "the survivor's own, two moved and the account"
    );
    assert!(moved_each.iter().all(Option::is_some), "{moved_each:?}");
    assert!(
        moved_each
            .iter()
            .flatten()
            .any(|fact| !fact.refs.is_empty()),
        "a folded claim's own reference reads on the survivor: {moved_each:?}"
    );
    assert!(
        moved_each
            .iter()
            .flatten()
            .any(|fact| !fact.stands_for.is_empty()),
        "and so does its own mark: {moved_each:?}"
    );
    assert!(moved_own.is_some());
    assert!(list(merged.folded_key.clone()).await.is_empty());

    merged.leave_in_place().await;

    // The folded rows are where they were made, and nothing is under the new ids.
    let stored_under_folded: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fact_write WHERE entity = ?")
            .bind(merged.folded_key.as_str())
            .fetch_one(&pool)
            .await
            .expect("counts");
    assert!(
        stored_under_folded > 0,
        "the rows are back under the folded key"
    );
    for table in ["fact_event_ref", "fact_stands_for"] {
        let own: i64 =
            sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE fact_home = ?"))
                .bind(merged.folded_key.as_str())
                .fetch_one(&pool)
                .await
                .expect("counts");
        assert!(
            own > 0,
            "{table}: the claim's own rows are back under the folded key"
        );
    }

    assert_eq!(list(merged.survivor_key.clone()).await, moved_list);
    for ((_, now), control) in merged.moved.iter().zip(&moved_each) {
        assert_eq!(&read(served_as(now)).await, control, "{now}");
    }
    assert_eq!(
        read(FactAddress::new(
            merged.survivor_key.clone(),
            merged.own.local.clone()
        ))
        .await,
        moved_own,
        "the survivor's own claim reads as it did"
    );
    assert!(
        list(merged.folded_key.clone()).await.is_empty(),
        "the folded thing lists nothing of its own"
    );
    for (was, _) in &merged.moved {
        assert_eq!(
            read(FactAddress::new(
                merged.folded_key.clone(),
                FactId(was.clone())
            ))
            .await,
            None,
            "{was}: the old address answers nothing under the folded key"
        );
    }
    store.stop().await;
}

/// **A merge whose field writes were left in place reads, as to its fields,
/// exactly like a merge whose field writes were moved** — what the survivor
/// holds, how many writes stand behind it, how old each value is, the trail of
/// every key and one key's history, and the folded thing holding nothing.
/// The values are checked, so a case that read nothing cannot pass for one
/// that read the same.
#[tokio::test]
async fn fields_left_in_place_by_a_merge_read_like_fields_moved_by_it() {
    let scratch = Scratch::new("merged-fields");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "merged_fields")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let merged = Merged::make(pool.clone()).await;

    // Everything the field families answer, for both handles, as one value.
    let answers = || async {
        let mut said = Vec::new();
        for handle in [&merged.survivor, &merged.folded] {
            said.push(format!("{:?}", merged.memory.fields(handle).await));
            said.push(format!(
                "{:?}",
                merged.memory.fields_versioned(handle).await
            ));
            said.push(format!("{:?}", merged.memory.field_ages(handle).await));
            said.push(format!("{:?}", merged.memory.key_trails(handle).await));
            for key in ["mood", "lunch", "city", "job"] {
                said.push(format!("{:?}", merged.memory.history(handle, key).await));
            }
        }
        said
    };
    let moved = answers().await;

    // The control holds what it should: the folded thing's `job` reaches the
    // survivor, its later `lunch` wins, and the survivor's later `mood` does.
    let held = merged
        .memory
        .fields(&merged.survivor)
        .await
        .expect("the fields read");
    assert_eq!(
        held.get("job").map(String::as_str),
        Some("clerk"),
        "{held:?}"
    );
    assert_eq!(
        held.get("lunch").map(String::as_str),
        Some("salad"),
        "{held:?}"
    );
    assert_eq!(
        held.get("mood").map(String::as_str),
        Some("cheerful"),
        "{held:?}"
    );
    assert_eq!(
        held.get("city").map(String::as_str),
        Some("Springfield"),
        "{held:?}"
    );
    let moods = merged
        .memory
        .history(&merged.survivor, "mood")
        .await
        .expect("the history reads");
    assert_eq!(
        moods.iter().map(|w| w.value.as_deref()).collect::<Vec<_>>(),
        [Some("calm"), Some("angry"), Some("cheerful")],
        "the writes of a key stand in the order they were made"
    );
    assert!(
        merged
            .memory
            .fields(&merged.folded)
            .await
            .expect("the fields read")
            .is_empty()
    );

    merged.leave_in_place().await;

    let stored_under_folded: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM field_write WHERE entity = ?")
            .bind(merged.folded_key.as_str())
            .fetch_one(&pool)
            .await
            .expect("counts");
    assert!(
        stored_under_folded > 0,
        "the writes are back under the folded key"
    );

    let left = answers().await;
    for (moved, left) in moved.iter().zip(&left) {
        assert_eq!(left, moved);
    }
    store.stop().await;
}

/// **A merge whose claims were left in place reads, as to the history of its
/// claims, exactly like a merge whose claims were moved** — one claim's writes
/// by the address it is served at and by the address it had before the merge,
/// every claim's writes at once, and the folded thing holding none. The folded
/// claim written twice is read to have both writes, so a case that read an
/// empty chain on both sides cannot pass.
#[tokio::test]
async fn claim_histories_left_in_place_by_a_merge_read_like_ones_moved_by_it() {
    let scratch = Scratch::new("merged-histories");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "merged_histories")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let merged = Merged::make(pool.clone()).await;

    let answers = || async {
        let mut said = Vec::new();
        let mut addresses = vec![merged.own.clone()];
        for (was, now) in &merged.moved {
            addresses.push(FactAddress::new(
                merged.survivor.clone(),
                FactId(now.clone()),
            ));
            addresses.push(FactAddress::new(merged.folded.clone(), FactId(was.clone())));
        }
        for address in &addresses {
            said.push(format!("{:?}", merged.memory.claim_history(address).await));
        }
        for handle in [&merged.survivor, &merged.folded] {
            let histories = merged.memory.claim_histories(handle).await;
            said.push(format!(
                "{:?}",
                histories.map(|all| {
                    let mut all: Vec<_> = all.into_iter().collect();
                    all.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
                    all
                })
            ));
        }
        said
    };
    let moved = answers().await;

    // The twice-written folded claim has both writes behind its new address.
    let (_, now) = merged.moved.last().expect("a moved claim");
    let chain = merged
        .memory
        .claim_history(&FactAddress::new(
            merged.survivor.clone(),
            FactId(now.clone()),
        ))
        .await
        .expect("the history reads");
    assert_eq!(
        chain.iter().map(|w| w.content.as_str()).collect::<Vec<_>>(),
        ["closes at night", "closes at midnight"]
    );
    assert!(
        merged
            .memory
            .claim_histories(&merged.folded)
            .await
            .expect("the histories read")
            .is_empty()
    );

    merged.leave_in_place().await;

    let left = answers().await;
    assert_eq!(left.len(), moved.len());
    for (moved, left) in moved.iter().zip(&left) {
        assert_eq!(left, moved);
    }
    store.stop().await;
}

/// **A merge whose claims were left in place answers the thought-room and
/// bag-of-keys questions of its claims exactly as a merge that moved them** —
/// when each claim was last touched, and which of a claim's keys stay off the
/// thing. The folded claim carrying a key of its own is read to keep it off, so
/// a case that found nothing on both sides cannot pass.
#[tokio::test]
async fn touch_moments_and_kept_off_keys_left_in_place_by_a_merge_read_like_moved_ones() {
    let scratch = Scratch::new("merged-touches");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "merged_touches")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let merged = Merged::make(pool.clone()).await;

    let answers = || async {
        let mut ids = vec![merged.own.local.clone()];
        let mut served = Vec::new();
        for (_, now) in &merged.moved {
            ids.push(FactId(now.clone()));
            served.push(FactAddress::new(
                merged.survivor.clone(),
                FactId(now.clone()),
            ));
        }
        let mut tx = merged.pool.begin().await.expect("a transaction");
        let touched = DoltMemory::touched_moments(&mut tx, &merged.survivor_key, &ids)
            .await
            .expect("touches read");
        tx.commit().await.expect("the read commits");
        let mut touched: Vec<_> = touched.into_iter().collect();
        touched.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
        let mut said = vec![format!("{touched:?}")];
        for address in &served {
            said.push(format!(
                "{:?}",
                merged.memory.keys_kept_off_the_thing(address).await
            ));
        }
        said
    };
    let moved = answers().await;
    assert_eq!(moved[0].matches("FactId").count(), 3, "{moved:?}");
    assert!(
        moved.iter().any(|said| said.contains("own_shift")),
        "a key of a claim's own stays off the thing: {moved:?}"
    );

    merged.leave_in_place().await;

    assert_eq!(answers().await, moved);
    store.stop().await;
}

/// **A list that named both sides of a merge reads as the survivor, once,
/// whether the merge rewrote the list or left it.** The values are read, so a
/// case that read nothing cannot pass for one that read the same.
#[tokio::test]
async fn a_list_naming_both_sides_of_a_merge_reads_the_survivor_once() {
    let scratch = Scratch::new("merged-lists");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "merged_lists")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let merged = Merged::make(pool.clone()).await;

    for placed in ["moved", "left"] {
        if placed == "left" {
            merged.leave_in_place().await;
        }
        let held = merged
            .memory
            .fields(&merged.pointing)
            .await
            .expect("the fields read");
        for key in ["friends", "regulars"] {
            assert_eq!(
                held.get(key).map(String::as_str),
                Some("person:homer"),
                "{placed}: {key}: {held:?}"
            );
        }
    }
    store.stop().await;
}

/// **A claim that points at a merged thing reads, as to those pointers, exactly
/// as it does when the merge rewrote them** — the edge, the reference, the field
/// naming the thing, the claim it was derived from and the mark naming that
/// claim all read as the survivor and the id the merge event gave it. The
/// control is read to say the survivor, so a case that read the folded thing
/// on both sides cannot pass.
#[tokio::test]
async fn pointers_left_naming_a_merged_thing_read_like_pointers_the_merge_rewrote() {
    let scratch = Scratch::new("merged-pointers");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "merged_pointers")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let merged = Merged::make(pool.clone()).await;

    let answers = || async {
        vec![
            format!("{:?}", merged.memory.recall(&merged.pointing).await),
            format!("{:?}", merged.memory.fields(&merged.pointing).await),
            format!("{:?}", merged.memory.key_trails(&merged.pointing).await),
            format!(
                "{:?}",
                merged.memory.history(&merged.pointing, "friend").await
            ),
            format!(
                "{:?}",
                merged.memory.claim_history(&merged.pointing_claim).await
            ),
            format!(
                "{:?}",
                merged.memory.claim_histories(&merged.pointing).await
            ),
        ]
    };
    let moved = answers().await;
    let claim = merged
        .memory
        .recall(&merged.pointing)
        .await
        .expect("recall reads")
        .into_iter()
        .next()
        .expect("the pointing claim");
    let (_, first_now) = &merged.moved[0];
    assert_eq!(
        claim.edge.map(|edge| edge.object),
        Some(merged.survivor.clone())
    );
    assert_eq!(claim.refs, vec![merged.survivor.clone()]);
    assert_eq!(
        claim.derived_from,
        Some(FactAddress::new(
            merged.survivor.clone(),
            FactId(first_now.clone())
        ))
    );
    assert_eq!(
        claim.stands_for,
        [FactAddress::new(
            merged.survivor.clone(),
            FactId(first_now.clone())
        )]
    );
    assert_eq!(
        claim.fields.get("friend").map(String::as_str),
        Some("person:homer")
    );

    merged.leave_in_place().await;

    // The pointers name the folded thing again, in the rows themselves.
    let edge: Option<String> = sqlx::query_scalar(
        "SELECT edge_object FROM fact WHERE entity = ? AND edge_object IS NOT NULL",
    )
    .bind(merged.pointing_key.as_str())
    .fetch_optional(&pool)
    .await
    .expect("the edge reads");
    assert_eq!(edge.as_deref(), Some(merged.folded_key.as_str()));
    let derived: Option<String> = sqlx::query_scalar(
        "SELECT derived_from FROM fact WHERE entity = ? AND derived_from IS NOT NULL",
    )
    .bind(merged.pointing_key.as_str())
    .fetch_optional(&pool)
    .await
    .expect("the lineage reads");
    assert_eq!(derived.as_deref(), Some(merged.folded_key.as_str()));

    let left = answers().await;
    for (moved, left) in moved.iter().zip(&left) {
        assert_eq!(left, moved);
    }
    store.stop().await;
}

/// **What points at a merged thing is found at the survivor, whether the merge
/// rewrote the pointers or left them** — the claims derived from one of its
/// claims, by the address it is served at and by the one it had, the claims
/// whose fields name it, and the bots that drew an edge at it. The folded thing
/// is pointed at by nothing. The control is read to find the claim, so a case
/// that found nothing on both sides cannot pass.
#[tokio::test]
async fn what_points_at_a_merged_thing_is_found_at_the_survivor_either_way() {
    let scratch = Scratch::new("merged-inbound");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "merged_inbound")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let merged = Merged::make(pool.clone()).await;

    let answers = || async {
        let (was, now) = &merged.moved[0];
        let mut said = Vec::new();
        for address in [
            FactAddress::new(merged.survivor.clone(), FactId(now.clone())),
            FactAddress::new(merged.folded.clone(), FactId(was.clone())),
        ] {
            said.push(format!("{:?}", merged.memory.built_on(&address).await));
        }
        for handle in [&merged.survivor, &merged.folded] {
            said.push(format!("{:?}", merged.memory.referring_to(handle).await));
            said.push(format!("{:?}", merged.memory.thought_writers(handle).await));
        }
        said
    };
    let moved = answers().await;
    assert!(
        moved[0].contains("knows the grocer") && moved[2].contains("knows the grocer"),
        "the claim is found by what it was derived from and by its field: {moved:?}"
    );
    assert!(moved[3].contains("bot:moe"), "{moved:?}");

    merged.leave_in_place().await;

    assert_eq!(answers().await, moved);
    store.stop().await;
}

/// **The chart reads the same whether a merge moved the folded thing's writes
/// or left them** — the folded thing reported to a bot, so after the merge the
/// survivor does: that bot heads the chart and has a report. Both are read
/// from the bots that ever wrote a manager, and those are read off the writes
/// where they are stored.
#[tokio::test]
async fn the_chart_reads_the_same_whether_a_merge_moved_the_writes_or_left_them() {
    let scratch = Scratch::new("merged-chart");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "merged_chart")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let merged = Merged::make(pool.clone()).await;

    let answers = || async {
        let mut tx = merged.pool.begin().await.expect("a transaction");
        let heads = merged.memory.chart_heads_in(&mut tx).await;
        let reports = merged
            .memory
            .has_reports_in(&mut tx, &merged.pointing)
            .await;
        tx.commit().await.expect("the read commits");
        vec![format!("{heads:?}"), format!("{reports:?}")]
    };
    let moved = answers().await;
    assert_eq!(moved[0], format!("Ok([{:?}])", merged.pointing));
    assert_eq!(moved[1], "Ok(true)");

    merged.leave_in_place().await;

    assert_eq!(answers().await, moved);
    store.stop().await;
}

/// **A merge that left the folded thing's names and its children alone reads,
/// as to those, exactly like a merge that moved them** — the survivor answers to
/// the folded thing's names, the folded thing to none, the child is the
/// survivor's, and the document the search index is built from says the same.
#[tokio::test]
async fn names_and_children_left_by_a_merge_read_like_ones_it_moved() {
    let scratch = Scratch::new("merged-names");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "merged_names")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let merged = Merged::make(pool.clone()).await;

    let answers = || async {
        let mut said = vec![
            format!("{:?}", merged.memory.list_entities(None).await),
            format!("{:?}", merged.memory.children(&merged.survivor).await),
            format!("{:?}", merged.memory.children(&merged.folded).await),
        ];
        let child = EntityId("place:leftorium".into());
        for handle in [&merged.survivor, &merged.folded, &child] {
            said.push(format!("{:?}", merged.memory.scan_entity(handle).await));
        }
        said.push(format!("{:?}", merged.memory.scan().await));
        said
    };
    let moved = answers().await;
    assert!(
        moved[1].contains("place:leftorium") && moved[2] == "Ok([])",
        "the child is the survivor's: {moved:?}"
    );
    assert!(
        moved[0].contains("Squishee Man"),
        "the survivor answers to the folded thing's alias: {moved:?}"
    );

    merged.leave_in_place().await;

    let parent: Option<String> = sqlx::query_scalar("SELECT parent FROM entity WHERE id = ?")
        .bind("place:leftorium")
        .fetch_one(&pool)
        .await
        .expect("the parent reads");
    assert_eq!(parent.as_deref(), Some(merged.folded_key.as_str()));
    let left = answers().await;
    for (moved, left) in moved.iter().zip(&left) {
        assert_eq!(left, moved);
    }
    store.stop().await;
}

/// **A chain of two merges, both of which left their rows in place, reads like
/// the same two merges when they moved them** — the thing folded first into a
/// second thing that was folded into a third. A claim of the first is served
/// at the id the second merge gave it, a field written by each of the three is
/// placed by the moment it was made, and the middle thing holds nothing.
#[tokio::test]
async fn a_chain_of_merges_left_in_place_reads_like_the_chain_moved() {
    let scratch = Scratch::new("merged-chain");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "merged_chain")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    let last = EntityId::person("person:homer");
    let middle = EntityId::person("person:apu");
    let first = EntityId::person("person:lisa");
    for (handle, name) in [(&last, "Homer"), (&middle, "Apu"), (&first, "Lisa")] {
        memory
            .add_entity(NewEntity::new(handle.clone(), name, "fixture"))
            .await
            .expect("add_entity ok")
            .written()
            .expect("the guard waves it through");
    }
    for (about, what, fields) in [
        (
            &last,
            "keeps bees",
            vec![("city", "Springfield"), ("mood", "calm")],
        ),
        (
            &first,
            "plays the saxophone",
            vec![("mood", "bored"), ("hobby", "jazz")],
        ),
        (&middle, "runs the store", vec![("mood", "angry")]),
    ] {
        let mut fact = NewFact::about(about.clone(), what, date(2026, 9, 1));
        fact.fields = fields
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        fact.sets = fact.fields.keys().cloned().collect();
        memory
            .capture(fact)
            .await
            .expect("capture ok")
            .written()
            .expect("the guard waves it through");
    }
    for (gone, kept) in [(&first, &middle), (&middle, &last)] {
        memory
            .merge(
                gone,
                kept,
                Some("filed twice"),
                date(2026, 9, 2),
                &EntityId("bot:sigma".into()),
                Some("run-a"),
            )
            .await
            .expect("the fold lands");
    }
    let mut tx = pool.begin().await.expect("a transaction");
    let mut keys = Vec::new();
    for handle in [&last, &middle, &first] {
        keys.push(
            memory
                .resolve(&mut tx, handle)
                .await
                .expect("resolve ok")
                .expect("the thing exists")
                .0,
        );
    }
    tx.commit().await.expect("the read commits");
    let (last_key, middle_key, first_key) = (keys[0].clone(), keys[1].clone(), keys[2].clone());
    let events = memory
        .merge_events(Some(&middle))
        .await
        .expect("events read");
    assert_eq!(events.len(), 2, "{events:?}");
    let moved = |event: &jojobot_domain::memory::MergeEvent| -> Vec<(String, String)> {
        event
            .claims
            .iter()
            .map(|(was, now)| (was.as_str().to_string(), now.as_str().to_string()))
            .collect()
    };
    let (first_moved, second_moved) = (moved(&events[0]), moved(&events[1]));

    let answers = || async {
        let mut said = vec![
            format!("{:?}", memory.recall(&last).await),
            format!("{:?}", memory.fields(&last).await),
            format!("{:?}", memory.fields_versioned(&last).await),
            format!("{:?}", memory.key_trails(&last).await),
            format!("{:?}", memory.history(&last, "mood").await),
            format!(
                "{:?}",
                memory.claim_histories(&last).await.map(|all| {
                    let mut all: Vec<_> = all.into_iter().collect();
                    all.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
                    all
                })
            ),
        ];
        for gone in [&middle, &first] {
            said.push(format!("{:?}", memory.recall(gone).await));
            said.push(format!("{:?}", memory.fields(gone).await));
        }
        said
    };
    let left_answers = answers().await;
    let held = memory.fields(&last).await.expect("the fields read");
    assert_eq!(
        held.get("mood").map(String::as_str),
        Some("angry"),
        "{held:?}"
    );
    assert_eq!(
        held.get("hobby").map(String::as_str),
        Some("jazz"),
        "{held:?}"
    );
    assert_eq!(
        memory.recall(&last).await.expect("recall reads").len(),
        5,
        "the survivor's claim, the two folded claims and the two accounts"
    );

    // The earlier merge moves its rows first, then the later one.
    move_rows_as_the_older_merge_did(&pool, &first_key, &middle_key, &first_moved).await;
    move_rows_as_the_older_merge_did(&pool, &middle_key, &last_key, &second_moved).await;

    let moved_answers = answers().await;
    assert_eq!(moved_answers.len(), left_answers.len());
    for (left, moved) in left_answers.iter().zip(&moved_answers) {
        assert_eq!(moved, left);
    }
    store.stop().await;
}

/// **The fold the server answers every read from holds a merge that left its
/// rows in place as it holds one that moved them.** The fold is built from the
/// store's scan and its other whole-thing reads, so each of them must carry the
/// survivor's claims, fields, histories and pointers. The control is the fold
/// built over the moved shape; the case is the fold built again over the same
/// store with the rows put back under the folded key.
#[tokio::test]
async fn the_fold_built_over_rows_left_in_place_answers_as_over_rows_moved() {
    let scratch = Scratch::new("merged-fold");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "merged_fold")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let merged = Merged::make(pool.clone()).await;
    let folded_memory = crate::fold::Folded::new(std::sync::Arc::new(merged.memory.clone()));
    folded_memory.rebuild().await.expect("the fold loads");

    let answers = || async {
        let mut said = Vec::new();
        for handle in [&merged.survivor, &merged.folded, &merged.pointing] {
            said.push(format!("{:?}", folded_memory.recall(handle).await));
            said.push(format!("{:?}", folded_memory.fields(handle).await));
            said.push(format!(
                "{:?}",
                folded_memory.fields_versioned(handle).await
            ));
            said.push(format!("{:?}", folded_memory.key_trails(handle).await));
            said.push(format!(
                "{:?}",
                folded_memory.claim_histories(handle).await.map(|all| {
                    let mut all: Vec<_> = all.into_iter().collect();
                    all.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
                    all
                },)
            ));
            said.push(format!("{:?}", folded_memory.referring_to(handle).await));
            said.push(format!("{:?}", folded_memory.scan_entity(handle).await));
        }
        for (was, now) in &merged.moved {
            for address in [
                FactAddress::new(merged.survivor.clone(), FactId(now.clone())),
                FactAddress::new(merged.folded.clone(), FactId(was.clone())),
            ] {
                said.push(format!("{:?}", folded_memory.claim_history(&address).await));
                said.push(format!("{:?}", folded_memory.built_on(&address).await));
            }
        }
        said.push(format!("{:?}", folded_memory.list_entities(None).await));
        said
    };
    let moved = answers().await;
    assert!(
        moved.iter().any(|said| said.contains("closes at midnight")),
        "the fold holds the folded thing's claim on the survivor"
    );

    merged.leave_in_place().await;
    // Writes made outside the server are not seen by a fold already loaded.
    folded_memory.rebuild().await.expect("the fold loads again");

    let left = answers().await;
    assert_eq!(left.len(), moved.len());
    for (index, (moved, left)) in moved.iter().zip(&left).enumerate() {
        assert_eq!(left, moved, "answer {index}");
    }
    store.stop().await;
}

/// **An edit that names the revision it read is judged against the claim's real
/// writes, whether a merge moved the claim or left it** — the folded claim
/// written twice is refused at a stale revision, and the refusal names the same
/// current revision in both shapes. The refusal is read to name two, so a case
/// that counted nothing in both shapes cannot pass.
#[tokio::test]
async fn a_stale_revision_is_counted_on_a_claim_left_in_place_as_on_one_moved() {
    let scratch = Scratch::new("merged-revision");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "merged_revision")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let merged = Merged::make(pool.clone()).await;
    let (_, now) = merged.moved.last().expect("a moved claim");
    let twice = FactAddress::new(merged.survivor.clone(), FactId(now.clone()));

    let refusal = || async {
        let patch = FactPatch {
            content: Some("closes at dawn".into()),
            expected_revision: Some(1),
            ..Default::default()
        };
        format!(
            "{:?}",
            merged
                .memory
                .update_fact(&twice, patch, &EntityId("bot:sigma".into()))
                .await
        )
    };
    let moved = refusal().await;
    assert!(
        moved.contains("StaleRevision") && moved.contains('2'),
        "{moved}"
    );

    merged.leave_in_place().await;

    assert_eq!(refusal().await, moved);
    store.stop().await;
}

/// What one write of a claim kept: its references and its mark, as handles.
type KeptMarks = (Option<Vec<String>>, Option<Vec<String>>);

/// The refs and the mark of every write of a claim, as the claim's history
/// serves them: one entry per write.
fn kept_marks_of(writes: &[ClaimWrite]) -> Vec<KeptMarks> {
    writes
        .iter()
        .map(|write| {
            (
                write
                    .refs
                    .as_ref()
                    .map(|refs| refs.iter().map(|r| r.as_str().to_string()).collect()),
                write.stands_for.as_ref().map(|marked| {
                    marked
                        .iter()
                        .map(|a| format!("{}#{}", a.home.as_str(), a.local.as_str()))
                        .collect()
                }),
            )
        })
        .collect()
}

/// **A claim's history names what the claim touched as it is now, through a
/// merge.** The claim on the third thing pointed at the folded thing in a
/// reference, and a later write marked the folded thing's first claim. The
/// snapshot each write kept names the folded thing, and the history serves the
/// survivor and the id the merge gave that claim, in the shape that moved the
/// folded claims and in the shape that left them. Paired: the survivor is named
/// and the folded thing is not.
#[tokio::test]
async fn a_claims_history_serves_its_kept_marks_through_a_merge() {
    let scratch = Scratch::new("merged-kept-marks");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "merged_kept_marks")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let merged = Merged::make(pool.clone()).await;
    assert!(
        merged.moved.contains(&("f1".to_string(), "f3".to_string())),
        "the folded thing's first claim is the survivor's third: {:?}",
        merged.moved
    );

    let read = || async {
        kept_marks_of(
            &merged
                .memory
                .claim_history(&merged.pointing_claim)
                .await
                .expect("the history reads"),
        )
    };
    let moved = read().await;
    assert_eq!(
        moved,
        vec![
            (Some(vec!["person:homer".to_string()]), Some(Vec::new())),
            (
                Some(vec!["person:homer".to_string()]),
                Some(vec!["person:homer#f3".to_string()])
            ),
        ]
    );

    merged.leave_in_place().await;

    assert_eq!(read().await, moved);
    store.stop().await;
}

/// **The history follows a second merge too**: the survivor is folded into a
/// third thing, and the claim's history names that thing and the id its first
/// claim has there.
#[tokio::test]
async fn a_claims_history_serves_its_kept_marks_through_a_chain_of_merges() {
    let scratch = Scratch::new("merged-kept-marks-chain");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "merged_kept_marks_chain")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let merged = Merged::make(pool.clone()).await;
    let lisa = EntityId::person("person:lisa");
    merged
        .memory
        .add_entity(NewEntity::new(lisa.clone(), "Lisa", "fixture"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("the guard waves it through");
    merged
        .memory
        .merge(
            &merged.survivor,
            &lisa,
            Some("filed twice again"),
            date(2026, 9, 3),
            &merged.pointing,
            Some("run-b"),
        )
        .await
        .expect("the second fold lands");

    let history = kept_marks_of(
        &merged
            .memory
            .claim_history(&merged.pointing_claim)
            .await
            .expect("the history reads"),
    );
    assert_eq!(
        history,
        vec![
            (Some(vec!["person:lisa".to_string()]), Some(Vec::new())),
            (
                Some(vec!["person:lisa".to_string()]),
                Some(vec!["person:lisa#f3".to_string()])
            ),
        ]
    );
    store.stop().await;
}

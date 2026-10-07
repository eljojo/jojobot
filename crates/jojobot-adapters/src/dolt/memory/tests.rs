use super::*;
use crate::dolt::tests::{Scratch, free_port};
use crate::dolt::{Dolt, migrate};
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
    let pool = store
        .database("projection")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    // The kinds, or no handle parses and every write is refused.
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");

    let subject = EntityId::person("person:projected");
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
    let pool = store
        .database("hotclaim")
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
    let pool = store
        .database("badgecollisionretry")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");

    let memory = DoltMemory::open(pool.clone());
    let first = EntityId::person("person:badge-collision-holder");
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
    let second = EntityId::person("person:badge-collision-retried");
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
    let pool = store
        .database("badgecollisiongiveup")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");

    let memory = DoltMemory::open(pool.clone());
    let first = EntityId::person("person:badge-collision-blocker");
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
    let second = EntityId::person("person:badge-collision-stuck");
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
    let pool = store
        .database("currenthandle")
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
        .rename_entity(&was, &now, None, date(2026, 8, 10), None)
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
    let pool = store
        .database("roleclaims")
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

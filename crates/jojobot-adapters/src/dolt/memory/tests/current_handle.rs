//! `current_handle` against the real store: the contract's own writes dealt in
//! parts, and one focused control that keeps the guards a part cannot.
//!
//! **`current_handle` must answer exactly what `entity_wearing` over the full
//! `known` list would answer, for every badge a real corpus holds.** The
//! contract's run leaves behind entities, renames and former handles, which is
//! everything `current_handle` exists to resolve. Comparing its answer with the
//! list-search it replaces, badge by badge, whole value against whole value, is
//! what catches a fast path that gets the common row right and a collision or a
//! never-badged handle wrong.

use super::*;
use jojobot_domain::memory::testing::contract;
use jojobot_domain::memory::{Guarded, guard};

/// A store and a database of this case's own, with the kinds seeded.
async fn a_store_of_its_own(tag: &str) -> (Scratch, Dolt, sqlx::MySqlPool) {
    let scratch = Scratch::new(&format!("current-handle-{tag}"));
    let store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, &format!("currenthandle{tag}"))
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    (scratch, store, pool)
}

/// **Every entity and every badge the store holds, asked of `current_handle` and
/// of `entity_wearing`.** The candidates always include the entity's own handle
/// and its badge when it wears one; the answers are compared whole. Returns how
/// many candidates were compared and how many entities the store held.
async fn compared_with_entity_wearing(
    memory: &DoltMemory,
    pool: &sqlx::MySqlPool,
) -> (usize, usize) {
    let mut tx = pool.begin().await.expect("a transaction");
    let known = memory.known(&mut tx).await.expect("the known rows read");
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
    tx.commit().await.expect("the read commits");
    (checked, known.len())
}

/// **One part of the contract, then the comparison over what that part left.**
async fn current_handle_after_part(part: usize) {
    let (_scratch, mut store, pool) = a_store_of_its_own(&format!("part{part}")).await;
    let memory = DoltMemory::open(pool.clone());
    let expected = contract::part_share(contract::case_count(), part);
    let ran = contract::run_part(&memory, part, contract::PARTS).await;
    contract::assert_part_ran("current_handle", part, ran, expected);

    let (checked, known) = compared_with_entity_wearing(&memory, &pool).await;
    assert!(
        checked >= known && checked > 0,
        "part {part} compared {checked} candidates over {known} entities",
    );
    store.stop().await;
}

/// **The parts cover every case of the contract once.** Pure: no store.
#[test]
fn the_current_handle_parts_cover_every_case() {
    contract::parts_cover_every_case(contract::PARTS);
}

jojobot_domain::contract_parts!(tokio::test, current_handle_after_part:
    current_handle_agrees_with_entity_wearing_for_every_badge_the_real_store_holds_part_00 = 0,
    current_handle_agrees_with_entity_wearing_for_every_badge_the_real_store_holds_part_01 = 1,
    current_handle_agrees_with_entity_wearing_for_every_badge_the_real_store_holds_part_02 = 2,
    current_handle_agrees_with_entity_wearing_for_every_badge_the_real_store_holds_part_03 = 3,
    current_handle_agrees_with_entity_wearing_for_every_badge_the_real_store_holds_part_04 = 4,
    current_handle_agrees_with_entity_wearing_for_every_badge_the_real_store_holds_part_05 = 5,
    current_handle_agrees_with_entity_wearing_for_every_badge_the_real_store_holds_part_06 = 6,
    current_handle_agrees_with_entity_wearing_for_every_badge_the_real_store_holds_part_07 = 7,
    current_handle_agrees_with_entity_wearing_for_every_badge_the_real_store_holds_part_08 = 8,
    current_handle_agrees_with_entity_wearing_for_every_badge_the_real_store_holds_part_09 = 9,
    current_handle_agrees_with_entity_wearing_for_every_badge_the_real_store_holds_part_10 = 10,
    current_handle_agrees_with_entity_wearing_for_every_badge_the_real_store_holds_part_11 = 11,
    current_handle_agrees_with_entity_wearing_for_every_badge_the_real_store_holds_part_12 = 12,
    current_handle_agrees_with_entity_wearing_for_every_badge_the_real_store_holds_part_13 = 13,
    current_handle_agrees_with_entity_wearing_for_every_badge_the_real_store_holds_part_14 = 14,
    current_handle_agrees_with_entity_wearing_for_every_badge_the_real_store_holds_part_15 = 15,
);

/// **The focused control: a corpus of more than fifty ordinary entities, the
/// whole comparison over it, a value nobody wears, and a rename.** The parts hold
/// the contract's comparisons; this holds the two floors a part cannot (more than
/// fifty entities known, more than fifty candidates compared), the unchanged
/// answer for a missing handle, and the one case that exercises "resolves to the
/// handle it wears NOW" rather than "resolves to whichever handle it always
/// wore": the badge does not move when the handle does.
#[tokio::test]
async fn current_handle_compares_a_nontrivial_corpus_and_follows_a_rename() {
    let (_scratch, mut store, pool) = a_store_of_its_own("control").await;
    let memory = DoltMemory::open(pool.clone());

    // Entities of one family share words, and the guard asks to be told apart.
    for n in 0..51 {
        let slug = format!("contract-current-handle-corpus-{n}");
        let new = NewEntity::new(
            EntityId(format!("thing:{slug}")),
            format!("Corpus member {n}"),
            "contract-fixture",
        );
        match memory.add_entity(new.clone()).await.expect("add_entity ok") {
            Guarded::Blocked {
                attempted,
                candidates,
            } => memory
                .add_entity(NewEntity {
                    override_token: Some(guard::override_token(&attempted, &candidates)),
                    override_reason: Some("a fixture of the same family".into()),
                    ..new
                })
                .await
                .expect("add_entity again")
                .written()
                .expect("not blocked twice"),
            other => other.written().expect("written"),
        };
    }

    let (checked, known) = compared_with_entity_wearing(&memory, &pool).await;
    assert!(
        known > 50,
        "the corpus should hold more than 50 entities, it holds {known}"
    );
    assert!(
        checked > 50,
        "too few candidates were actually checked ({checked})",
    );

    let mut tx = pool.begin().await.expect("a transaction");
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
        .rename_entity(
            &was,
            &now,
            None,
            date(2026, 8, 10),
            jojobot_domain::memory::Override::none(),
            None,
        )
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

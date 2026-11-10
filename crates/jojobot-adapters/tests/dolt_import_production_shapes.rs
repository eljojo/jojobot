//! **The claims import over the merge shape a real store holds.** The recorded
//! fixture folds a thing that holds one claim or two; the store the import meets
//! in production holds ten merges, one that moved five claims and one that moved
//! none. The case builds that shape through the store's own merge route and checks
//! what the import does with it.

use jiff::{Timestamp, civil::date};
use jojobot_adapters::claims_import::{Imported, import_claims_once};
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, claims_events, free_port, migrated_database};
use jojobot_adapters::writer::Writer;
use jojobot_domain::log::claims::State;
use jojobot_domain::memory::owned::Provisions;
use jojobot_domain::memory::{EntityId, Guarded, Memory, NewEntity, NewFact, Provenance};

/// A migrated store of this case's own, with the shipped kinds.
async fn a_store(label: &str) -> (Scratch, Dolt, sqlx::MySqlPool) {
    let scratch = Scratch::new(label);
    let server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, label)
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    (scratch, server, pool)
}

async fn thing(memory: &DoltMemory, handle: &str, name: &str) -> EntityId {
    let id = EntityId(handle.to_string());
    memory
        .add_entity(NewEntity::new(id.clone(), name, "fixture"))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    id
}

fn at() -> Timestamp {
    Timestamp::from_second(1_800_000_000).expect("a moment")
}

/// **The claims each of production's ten merges moved**, in merge-number order.
/// Read from nelly's store on 2026-11-09, numbers only: `merge_event` held 26 rows,
/// merge numbers 1 to 10 with none missing, and the rows of each merge, its own
/// row first, numbered 2, 2, 2, 6, 2, 1, 3, 2, 3, 3. A merge that moved no claim
/// is one row, and one that moved five is six.
const CLAIMS_MOVED_BY_PRODUCTIONS_MERGES: [usize; 10] = [1, 1, 1, 5, 1, 0, 2, 1, 2, 2];

/// **The claims import takes the ten merges of the shape production holds.** The
/// recorded fixture folds a thing that holds one claim or two, five times; the
/// store the import meets holds ten merges, one that moved five claims and one that
/// moved none. The merges here are made by the store's own merge route, so the rows
/// are the rows it writes, and the case says first that they have production's
/// shape. Every claim a merge moved is then served by the survivor, as the old
/// rows serve it, and no merge number is left unexplained.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_takes_ten_merges_of_the_shape_production_holds() {
    let (_scratch, mut server, pool) = a_store("claims-import-production-merges").await;
    let memory = DoltMemory::open(pool.clone());
    let me = thing(&memory, "bot:sigma", "Sigma").await;
    let survivors = [
        thing(&memory, "person:homer", "Homer").await,
        thing(&memory, "person:marge", "Marge").await,
        thing(&memory, "person:bart", "Bart").await,
    ];
    let folded = [
        ("person:patty", "Patty"),
        ("person:selma", "Selma"),
        ("person:milhouse", "Milhouse"),
        ("person:nelson", "Nelson"),
        ("person:ralph", "Ralph"),
        ("person:apu", "Apu"),
        ("person:moe", "Moe"),
        ("person:lenny", "Lenny"),
        ("person:wiggum", "Wiggum"),
        ("person:otto", "Otto"),
    ];
    // The keys each survivor must serve once its merges are in: one per moved claim.
    let mut expected: Vec<Vec<String>> = vec![Vec::new(); survivors.len()];
    for (at, ((handle, name), moved)) in folded
        .iter()
        .zip(CLAIMS_MOVED_BY_PRODUCTIONS_MERGES)
        .enumerate()
    {
        // **A name the screen finds near one already held is blocked**, so the ten
        // names here are far from each other and from the survivors' (a short slug
        // one letter pair from another, or a name inside another, is refused).
        let duplicate = EntityId((*handle).to_string());
        match memory
            .add_entity(NewEntity::new(duplicate.clone(), *name, "fixture"))
            .await
            .expect("add")
        {
            Guarded::Written(_) => {}
            blocked => panic!("{handle} was blocked: {blocked:?}"),
        }
        for claim in 0..moved {
            let key = format!("moved_{at}_{claim}");
            let mut fact = NewFact::about(
                duplicate.clone(),
                "a claim the merge moves",
                date(2026, 9, 1),
            );
            fact.provenance = Provenance::Testimony;
            fact.fields = [(key.clone(), "kept".to_string())].into_iter().collect();
            fact.sets = fact.fields.keys().cloned().collect();
            memory
                .capture(fact)
                .await
                .expect("capture")
                .written()
                .expect("not blocked");
            expected[at % survivors.len()].push(key);
        }
        memory
            .merge(
                &duplicate,
                &survivors[at % survivors.len()],
                Some("filed twice"),
                date(2026, 9, 3 + at as i8),
                &me,
                Some("run-a"),
            )
            .await
            .expect("the fold lands");
    }

    let rows: Vec<i64> =
        sqlx::query_scalar("SELECT COUNT(*) FROM merge_event GROUP BY merge_no ORDER BY merge_no")
            .fetch_all(&pool)
            .await
            .expect("the merge rows count");
    assert_eq!(
        rows,
        [2, 2, 2, 6, 2, 1, 3, 2, 3, 3],
        "the store the case makes has the rows production holds, merge by merge"
    );
    let mut old_fields = Vec::new();
    for survivor in &survivors {
        old_fields.push(memory.fields(survivor).await.expect("the old rows serve"));
    }

    let imported = import_claims_once(&Writer::default(), &pool, &Provisions::default(), at())
        .await
        .expect("ten merges of production's shape import");
    let Imported::Done {
        empty_merge_numbers,
        skipped_homes,
        skipped_key_writes,
        ..
    } = imported
    else {
        panic!("the import ran: {imported:?}");
    };
    assert!(
        empty_merge_numbers.is_empty(),
        "production holds merge numbers 1 to 10 and none is left unexplained: {empty_merge_numbers:?}"
    );
    assert!(skipped_homes.is_empty(), "{skipped_homes:?}");
    assert!(skipped_key_writes.is_empty(), "{skipped_key_writes:?}");

    let state = State::replay(claims_events(&pool).await.iter());
    for ((survivor, keys), old) in survivors.iter().zip(&expected).zip(&old_fields) {
        assert!(
            !keys.is_empty(),
            "{survivor} takes a merge that moved a claim"
        );
        let badge: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
            .bind(survivor.as_str())
            .fetch_one(&pool)
            .await
            .expect("a badge");
        let served = state.fields(&jojobot_domain::log::ObjectId(badge), &[]);
        for key in keys {
            assert_eq!(
                old.get(key).map(String::as_str),
                Some("kept"),
                "the old rows serve {key} on {survivor}"
            );
            assert_eq!(
                served.get(key).map(String::as_str),
                Some("kept"),
                "the import serves {key} on {survivor}, as the old rows do"
            );
        }
    }
    // **Each folded handle resolves to its survivor**, the merge that moved no claim
    // included: its own row is all the import has of that merge, so a claim that
    // arrives does not say the merge did.
    for (at, (handle, _)) in folded.iter().enumerate() {
        let badge: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
            .bind(survivors[at % survivors.len()].as_str())
            .fetch_one(&pool)
            .await
            .expect("a badge");
        let survivor = jojobot_domain::log::ObjectId(badge);
        assert_eq!(
            state.resolve(handle),
            Some(&survivor),
            "{handle} resolves to the survivor its merge named"
        );
    }
    server.stop().await;
}

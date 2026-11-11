//! **The claims import over the shapes a real store holds and the recorded fixture
//! does not.** The fixture folds a thing that holds one claim or two; the store the
//! import meets holds many merges, among them one that moved five claims and one that
//! moved none (the first case). It holds entities no build wrote a write row for, and
//! claims older than the table that records their key writes (the second to fourth).
//! The merges are made through the store's own route. A shape an old build left and no
//! route makes today is written as that build left it, and the case says so. The
//! days the rows are dated are the fixture's own, so no count reads the clock.

use jiff::{Timestamp, civil::date};
use jojobot_adapters::claims_import::{Imported, import_claims_once};
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, claims_events, free_port, migrated_database};
use jojobot_adapters::writer::Writer;
use jojobot_domain::log::claims::State;
use jojobot_domain::memory::owned::Provisions;
use jojobot_domain::memory::{
    EntityId, EntityPatch, Guarded, Memory, NewEntity, NewFact, Provenance,
};

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

/// **The claims each of ten merges moved**, in merge-number order. A merge writes one
/// row of its own and one for each claim it moves, so a merge that moved no claim is
/// one row and one that moved five is six.
const CLAIMS_MOVED_BY_THE_MERGES: [usize; 10] = [1, 1, 1, 5, 1, 0, 2, 1, 2, 2];

/// **The claims import takes ten merges of a mixed shape.** The recorded fixture
/// folds a thing that holds one claim or two, five times; a store that has run a
/// while holds more merges, one that moved five claims and one that moved none. The
/// merges here are made by the store's own merge route, so the rows are the rows it
/// writes, and the case says first what they are. Every claim a merge moved is then
/// served by the survivor, as the old rows serve it, and no merge number is left
/// unexplained.
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
    for (at, ((handle, name), moved)) in folded.iter().zip(CLAIMS_MOVED_BY_THE_MERGES).enumerate() {
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
        "the store the case makes has the rows of each merge, one of its own and one for each claim it moved"
    );
    let mut old_fields = Vec::new();
    for survivor in &survivors {
        old_fields.push(memory.fields(survivor).await.expect("the old rows serve"));
    }

    let imported = import_claims_once(&Writer::default(), &pool, &Provisions::default(), at())
        .await
        .expect("ten merges of a mixed shape import");
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
        "merge numbers 1 to 10 are all held and none is left unexplained: {empty_merge_numbers:?}"
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

/// The badge the store keeps an entity under; every write table is keyed by it.
async fn badge_of(pool: &sqlx::MySqlPool, handle: &str) -> String {
    sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(handle)
        .fetch_one(pool)
        .await
        .expect("a badge")
}

/// One claim that carries one key, as a caller writes it.
async fn say_with_key(memory: &DoltMemory, about: &EntityId, what: &str, key: &str) {
    let mut fact = NewFact::about(about.clone(), what, date(2026, 9, 1));
    fact.provenance = Provenance::Testimony;
    fact.fields = [(key.to_string(), "kept".to_string())]
        .into_iter()
        .collect();
    fact.sets = fact.fields.keys().cloned().collect();
    memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
}

/// **The claims import takes entities that have no `entity_write` row.** The write
/// rows are filed under the handle the entity wore, or under its badge. The import
/// has a path for an entity with no row at all (an entity row whose snapshot no build
/// kept), and no case ran it. Today's routes write a row for each entity, so the
/// rows of three are taken out the way an older build could have left them, and a
/// fourth keeps its own. The import then serves every entity, key and claim the old
/// rows served.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_takes_entities_that_have_no_entity_write_row() {
    let (_scratch, mut server, pool) = a_store("claims-import-no-entity-write").await;
    let memory = DoltMemory::open(pool.clone());
    let kept = "person:lenny";
    let all = ["person:homer", "person:marge", "person:bart", kept];
    for (handle, name) in [
        ("person:homer", "Homer"),
        ("person:marge", "Marge"),
        ("person:bart", "Bart"),
        (kept, "Lenny"),
    ] {
        let id = thing(&memory, handle, name).await;
        say_with_key(&memory, &id, &format!("{name} keeps a note"), "note").await;
    }
    memory
        .update_entity(
            &EntityId(kept.to_string()),
            EntityPatch {
                name: Some("Lenny Leonard".into()),
                ..Default::default()
            },
        )
        .await
        .expect("the rename lands");
    // The rows are filed under the handle the entity wore when they were written
    // (the badge only on a store that was moved to badges), so the count reads both.
    let rows_of = |handle: &'static str| {
        let pool = pool.clone();
        async move {
            let badge = badge_of(&pool, handle).await;
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM entity_write WHERE entity = ? OR entity = ?",
            )
            .bind(handle)
            .bind(badge)
            .fetch_one(&pool)
            .await
            .expect("the write rows count")
        }
    };
    for handle in all {
        assert!(
            rows_of(handle).await > 0,
            "today's routes write a row for {handle}"
        );
    }
    // Written as an older build could have left them: no row of the entity's own.
    for handle in all.iter().filter(|handle| **handle != kept) {
        sqlx::query("DELETE FROM entity_write WHERE entity = ? OR entity = ?")
            .bind(*handle)
            .bind(badge_of(&pool, handle).await)
            .execute(&pool)
            .await
            .expect("the write rows go");
    }
    for handle in all {
        assert_eq!(
            rows_of(handle).await > 0,
            handle == kept,
            "{handle}: only the entity that kept its rows has an entity_write row"
        );
    }
    let mut old = Vec::new();
    for handle in all {
        old.push(
            memory
                .fields(&EntityId(handle.to_string()))
                .await
                .expect("the old rows serve"),
        );
    }

    let imported = import_claims_once(&Writer::default(), &pool, &Provisions::default(), at())
        .await
        .expect("entities with no write row import");
    let Imported::Done {
        skipped_homes,
        skipped_key_writes,
        ..
    } = imported
    else {
        panic!("the import ran: {imported:?}");
    };
    assert!(skipped_homes.is_empty(), "{skipped_homes:?}");
    assert!(skipped_key_writes.is_empty(), "{skipped_key_writes:?}");

    let events = claims_events(&pool).await;
    let state = State::replay(events.iter());
    for (handle, old) in all.iter().zip(&old) {
        let badge = jojobot_domain::log::ObjectId(badge_of(&pool, handle).await);
        assert_eq!(
            state.resolve(handle),
            Some(&badge),
            "{handle} answers by its handle"
        );
        assert!(
            state.object(&badge).is_some(),
            "{handle} is an object of the State"
        );
        assert_eq!(
            &state.fields(&badge, &[]),
            old,
            "the import serves the keys of {handle} as the old rows do"
        );
        let note = state
            .active_records(&badge)
            .into_iter()
            .filter_map(|record| state.record(record))
            .any(|record| record.claim.content.ends_with("keeps a note"));
        assert!(note, "the import serves the claim of {handle}");
        // An entity with a write row is baselined at the moment of its first
        // write; one with none is baselined with no moment, and no moment is made up.
        let baselined = events
            .iter()
            .find(|event| {
                matches!(&event.payload,
                    jojobot_domain::log::claims::ClaimEvent::ObjectBaselined { object, .. }
                        if *object == badge)
            })
            .expect("the entity is baselined");
        assert_eq!(
            baselined.imported_source_time.is_some(),
            *handle == kept,
            "{handle}: the baseline holds a moment only when a write row gave one"
        );
    }
    server.stop().await;
}

/// **The claims import takes a claim older than the table that records its key
/// writes, and one whose first write has no time.** A store that has run a while holds
/// claims first written before key writes were dated, some with a key write and some
/// whose first write is undated. A key write older than the table that stamps them
/// has no moment and no claim-write number, so the rows of four claims are rewritten
/// as that build left them: dated early with a key write stamped later,
/// dated early with an unstamped key write, undated with an unstamped key write, and
/// undated with no key write. A fifth keeps today's rows. The import serves every
/// claim and key the old rows served.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_takes_a_claim_older_than_its_key_writes_and_one_with_no_first_time() {
    let (_scratch, mut server, pool) = a_store("claims-import-old-claims").await;
    let memory = DoltMemory::open(pool.clone());
    let homer = thing(&memory, "person:homer", "Homer").await;
    let badge = badge_of(&pool, "person:homer").await;
    say_with_key(&memory, &homer, "dated early, key stamped later", "mood_a").await;
    say_with_key(&memory, &homer, "dated early, key unstamped", "mood_b").await;
    say_with_key(&memory, &homer, "undated, key unstamped", "mood_c").await;
    memory
        .capture({
            let mut fact = NewFact::about(homer.clone(), "undated, no key", date(2026, 9, 1));
            fact.provenance = Provenance::Testimony;
            fact
        })
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
    say_with_key(&memory, &homer, "written today", "mood_e").await;

    let fact_id = |content: &'static str| {
        let pool = pool.clone();
        let badge = badge.clone();
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT fact_id FROM fact_write WHERE entity = ? AND content = ?",
            )
            .bind(badge)
            .bind(content)
            .fetch_one(&pool)
            .await
            .expect("the claim has a write row")
        }
    };
    for (content, first_write) in [
        (
            "dated early, key stamped later",
            Some("2026-09-05T10:00:00Z"),
        ),
        ("dated early, key unstamped", Some("2026-09-06T10:00:00Z")),
        ("undated, key unstamped", None),
        ("undated, no key", None),
    ] {
        sqlx::query("UPDATE fact_write SET written_at = ? WHERE entity = ? AND fact_id = ?")
            .bind(first_write)
            .bind(&badge)
            .bind(fact_id(content).await)
            .execute(&pool)
            .await
            .expect("the first write is dated as the old build left it");
    }
    sqlx::query(
        "UPDATE field_write SET written_at = '2026-10-09T10:00:00Z' \
         WHERE entity = ? AND fact_id = ?",
    )
    .bind(&badge)
    .bind(fact_id("dated early, key stamped later").await)
    .execute(&pool)
    .await
    .expect("a key write stamped after the claim");
    for content in ["dated early, key unstamped", "undated, key unstamped"] {
        sqlx::query(
            "UPDATE field_write SET written_at = NULL, fact_write_ordinal = NULL \
             WHERE entity = ? AND fact_id = ?",
        )
        .bind(&badge)
        .bind(fact_id(content).await)
        .execute(&pool)
        .await
        .expect("a key write older than the stamp and the claim-write number");
    }

    let older: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fact_write WHERE entity = ? AND written_at < '2026-10-08'",
    )
    .bind(&badge)
    .fetch_one(&pool)
    .await
    .expect("the early claims count");
    let undated: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fact_write WHERE entity = ? AND written_at IS NULL",
    )
    .bind(&badge)
    .fetch_one(&pool)
    .await
    .expect("the undated claims count");
    let bare_key_writes: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM field_write WHERE entity = ? AND written_at IS NULL \
         AND fact_write_ordinal IS NULL",
    )
    .bind(&badge)
    .fetch_one(&pool)
    .await
    .expect("the unstamped key writes count");
    assert_eq!(
        (older, undated, bare_key_writes),
        (2, 2, 2),
        "the store has the old shapes: claims dated before the first dated key \
         write, claims with no first time, and key writes with neither stamp nor number"
    );
    let old = memory.fields(&homer).await.expect("the old rows serve");
    assert_eq!(old.len(), 4, "the old rows serve the four keys");

    let imported = import_claims_once(&Writer::default(), &pool, &Provisions::default(), at())
        .await
        .expect("claims older than their key writes import");
    let Imported::Done {
        skipped_homes,
        skipped_key_writes,
        ..
    } = imported
    else {
        panic!("the import ran: {imported:?}");
    };
    assert!(skipped_homes.is_empty(), "{skipped_homes:?}");
    assert!(skipped_key_writes.is_empty(), "{skipped_key_writes:?}");

    let state = State::replay(claims_events(&pool).await.iter());
    let object = jojobot_domain::log::ObjectId(badge);
    assert_eq!(
        state.fields(&object, &[]),
        old,
        "the import serves the keys of the old claims as the old rows do"
    );
    let mut served: Vec<String> = state
        .active_records(&object)
        .into_iter()
        .filter_map(|record| state.record(record))
        .map(|record| record.claim.content.clone())
        .collect();
    served.sort();
    assert_eq!(
        served,
        [
            "dated early, key stamped later",
            "dated early, key unstamped",
            "undated, key unstamped",
            "undated, no key",
            "written today",
        ],
        "the import serves each claim, the undated ones included"
    );
    server.stop().await;
}

/// What the two cases below read: one entity, its badge, and a claim written
/// through the store's own route.
async fn homer_with_a_claim(
    label: &str,
    what: &str,
    key: Option<&str>,
) -> (Scratch, Dolt, sqlx::MySqlPool, DoltMemory, EntityId, String) {
    let (scratch, server, pool) = a_store(label).await;
    let memory = DoltMemory::open(pool.clone());
    let homer = thing(&memory, "person:homer", "Homer").await;
    let badge = badge_of(&pool, "person:homer").await;
    match key {
        Some(key) => say_with_key(&memory, &homer, what, key).await,
        None => {
            let mut fact = NewFact::about(homer.clone(), what, date(2026, 9, 1));
            fact.provenance = Provenance::Testimony;
            memory
                .capture(fact)
                .await
                .expect("capture")
                .written()
                .expect("not blocked");
        }
    }
    (scratch, server, pool, memory, homer, badge)
}

/// **The claims import takes a claim written before key writes were kept, that has
/// none.** A store that has run a while holds claims first written before the first
/// dated key write, with no key write row at all. The claim is made through the
/// store's own route and its first write is dated early; it carries no key, so it has
/// no `field_write` row. A claim written later, with a key, stands beside it. The
/// import serves both and the one key.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_takes_an_old_claim_that_has_no_key_write_at_all() {
    let (_scratch, mut server, pool, memory, homer, badge) =
        homer_with_a_claim("claims-import-old-no-key-write", "an old claim", None).await;
    say_with_key(&memory, &homer, "written today", "mood_e").await;
    sqlx::query("UPDATE fact_write SET written_at = '2026-09-01T00:00:00Z' WHERE entity = ? AND content = 'an old claim'")
        .bind(&badge)
        .execute(&pool)
        .await
        .expect("the first write is dated early");
    let key_writes_of_old: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM field_write w JOIN fact_write f \
         ON f.entity = w.entity AND f.fact_id = w.fact_id \
         WHERE w.entity = ? AND f.content = 'an old claim'",
    )
    .bind(&badge)
    .fetch_one(&pool)
    .await
    .expect("the key writes count");
    let early: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fact_write WHERE entity = ? AND written_at < '2026-10-08'",
    )
    .bind(&badge)
    .fetch_one(&pool)
    .await
    .expect("the early claims count");
    assert_eq!(
        (early, key_writes_of_old),
        (1, 0),
        "an old claim with no key write row at all"
    );
    let old = memory.fields(&homer).await.expect("the old rows serve");

    let imported = import_claims_once(&Writer::default(), &pool, &Provisions::default(), at())
        .await
        .expect("a claim with no key write imports");
    let Imported::Done {
        skipped_homes,
        skipped_key_writes,
        ..
    } = imported
    else {
        panic!("the import ran: {imported:?}");
    };
    assert!(skipped_homes.is_empty(), "{skipped_homes:?}");
    assert!(skipped_key_writes.is_empty(), "{skipped_key_writes:?}");

    let state = State::replay(claims_events(&pool).await.iter());
    let object = jojobot_domain::log::ObjectId(badge);
    let mut served: Vec<String> = state
        .active_records(&object)
        .into_iter()
        .filter_map(|record| state.record(record))
        .map(|record| record.claim.content.clone())
        .collect();
    served.sort();
    assert_eq!(served, ["an old claim", "written today"]);
    assert_eq!(state.fields(&object, &[]), old, "the one key is served");
    server.stop().await;
}

/// **The claims import takes a timed key write over a claim whose first write has no
/// time.** A store that has run a while holds claims whose key writes carry a time and
/// whose first write does not. A key write stamped before the claim-write number existed has
/// neither number nor a claim write to name, so the case writes the rows as that
/// build left them: the first write undated, the key write stamped and unnumbered.
/// The import serves the claim and its key.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_takes_a_timed_key_write_over_a_claim_whose_first_write_has_no_time() {
    let (_scratch, mut server, pool, memory, homer, badge) = homer_with_a_claim(
        "claims-import-timed-key-undated-claim",
        "an undated claim",
        Some("mood_f"),
    )
    .await;
    sqlx::query("UPDATE fact_write SET written_at = NULL WHERE entity = ?")
        .bind(&badge)
        .execute(&pool)
        .await
        .expect("the first write has no time");
    sqlx::query(
        "UPDATE field_write SET written_at = '2026-10-09T10:00:00Z', fact_write_ordinal = NULL \
         WHERE entity = ?",
    )
    .bind(&badge)
    .execute(&pool)
    .await
    .expect("the key write is stamped and unnumbered");
    let undated: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fact_write WHERE entity = ? AND written_at IS NULL",
    )
    .bind(&badge)
    .fetch_one(&pool)
    .await
    .expect("the undated claims count");
    let timed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM field_write WHERE entity = ? AND written_at IS NOT NULL \
         AND fact_write_ordinal IS NULL",
    )
    .bind(&badge)
    .fetch_one(&pool)
    .await
    .expect("the timed key writes count");
    assert_eq!(
        (undated, timed),
        (1, 1),
        "an undated first write beside a stamped, unnumbered key write"
    );
    let old = memory.fields(&homer).await.expect("the old rows serve");
    assert_eq!(old.len(), 1, "the old rows serve the key");

    let imported = import_claims_once(&Writer::default(), &pool, &Provisions::default(), at())
        .await
        .expect("a timed key write over an undated claim imports");
    let Imported::Done {
        skipped_homes,
        skipped_key_writes,
        ..
    } = imported
    else {
        panic!("the import ran: {imported:?}");
    };
    assert!(skipped_homes.is_empty(), "{skipped_homes:?}");
    assert!(skipped_key_writes.is_empty(), "{skipped_key_writes:?}");

    let state = State::replay(claims_events(&pool).await.iter());
    let object = jojobot_domain::log::ObjectId(badge);
    assert_eq!(state.fields(&object, &[]), old, "the key is served");
    assert!(
        state
            .active_records(&object)
            .into_iter()
            .filter_map(|record| state.record(record))
            .any(|record| record.claim.content == "an undated claim"),
        "the claim is served"
    );
    server.stop().await;
}

//! **Today's claims, fields and identities are imported into the event log once,
//! in the real store.** The rows are made with the port the way a deployed build
//! made them; the import reads them back, maps them and appends them as one
//! batch. What it appended is what the pure mapping says the rows mean, and
//! nothing is appended when a row is not understood.

use jiff::{Timestamp, civil::date};
use jojobot_adapters::claims_import::{ClaimsImportError, Imported, import_claims_once};
use jojobot_adapters::claims_log::codec;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_adapters::writer::Writer;
use jojobot_domain::log::Event;
use jojobot_domain::log::claims::import::import;
use jojobot_domain::log::claims::{ClaimEvent, State};
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact, Provenance};

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

async fn say(memory: &DoltMemory, about: &EntityId, what: &str) {
    let mut fact = NewFact::about(about.clone(), what, date(2026, 9, 1));
    fact.provenance = Provenance::Testimony;
    memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
}

/// A migrated store of this case's own, with the shipped kinds.
async fn store(label: &str) -> (Scratch, Dolt, sqlx::MySqlPool) {
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

/// Every claims event the log holds, in order, as stored.
async fn logged(pool: &sqlx::MySqlPool) -> Vec<Event<ClaimEvent>> {
    // Read with plain SQL: opening a second writer on the pool for every read
    // would hold a connection each.
    let payloads: Vec<Vec<u8>> = sqlx::query_scalar(
        "SELECT payload FROM event_log WHERE first_position IS NOT NULL ORDER BY first_position",
    )
    .fetch_all(pool)
    .await
    .expect("the log is readable");
    let mut events = Vec::new();
    for payload in payloads {
        // A stored batch is a tuple whose last element is its members, each a byte
        // array.
        let batch: serde_json::Value = serde_json::from_slice(&payload).expect("a stored batch");
        for member in batch[5].as_array().expect("a batch holds its members") {
            let bytes: Vec<u8> = member
                .as_array()
                .expect("a member is bytes")
                .iter()
                .map(|byte| byte.as_u64().expect("a byte") as u8)
                .collect();
            if let Some(event) = codec::decode_own(&bytes).expect("every stored event reads") {
                events.push(event);
            }
        }
    }
    events
}

/// What the mapping says today's rows mean, as the log stores it.
async fn expected(pool: &sqlx::MySqlPool) -> Vec<Event<ClaimEvent>> {
    let read = DoltMemory::open(pool.clone())
        .import_rows()
        .await
        .expect("the rows read");
    import(&read.rows)
        .expect("the rows map")
        .iter()
        .map(|event| codec::decode(&codec::encode(event)).expect("an event reads back"))
        .collect()
}

fn at() -> Timestamp {
    Timestamp::from_second(1_800_000_000).expect("a moment")
}

/// **The import appends what the rows mean, once.** A store with things, a claim,
/// a key written and a merge: the log then holds exactly the events the mapping
/// makes of those rows, the state they replay to answers the things by their
/// handles, and a second call reads nothing and appends nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_appends_what_the_rows_mean_once() {
    let (_scratch, mut server, pool) = store("claims-import-once").await;
    let memory = DoltMemory::open(pool.clone());
    let homer = thing(&memory, "person:homer", "Homer").await;
    let bart = thing(&memory, "person:bart", "Bart").await;
    let me = thing(&memory, "bot:sigma", "Sigma").await;
    say(&memory, &homer, "likes donuts").await;
    say(&memory, &bart, "skates").await;
    memory
        .merge(
            &bart,
            &homer,
            Some("filed twice"),
            date(2026, 9, 2),
            &me,
            None,
        )
        .await
        .expect("the merge");
    assert!(
        logged(&pool).await.is_empty(),
        "the log starts with no claims event"
    );

    let writer = Writer::default();
    let first = import_claims_once(&writer, &pool, at())
        .await
        .expect("the import runs");
    let Imported::Done { events } = first else {
        panic!("the first call imports: {first:?}");
    };
    assert!(events > 0);

    let held = logged(&pool).await;
    assert_eq!(held.len(), events, "the log holds what the call reports");
    assert_eq!(
        held,
        expected(&pool).await,
        "the log holds the events the mapping makes of the rows, in its order"
    );
    let state = State::replay(&held);
    assert!(
        state.resolve("person:homer").is_some(),
        "the state answers a thing by its handle"
    );

    let again = import_claims_once(&writer, &pool, at())
        .await
        .expect("the second call runs");
    assert_eq!(again, Imported::AlreadyDone);
    assert_eq!(
        logged(&pool).await.len(),
        events,
        "a second call appends nothing"
    );

    server.stop().await;
}

/// **A row the reader will not place stops the import with nothing appended.** An
/// entity whose parent names nothing the store holds is a finding; the import
/// names it and writes no event, and the next start meets the same rows.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_refuses_a_row_the_reader_will_not_place() {
    let (_scratch, mut server, pool) = store("claims-import-finding").await;
    let memory = DoltMemory::open(pool.clone());
    thing(&memory, "person:homer", "Homer").await;
    thing(&memory, "person:marge", "Marge").await;
    sqlx::query("UPDATE entity SET parent = 'person:moe' WHERE id = 'person:marge'")
        .execute(&pool)
        .await
        .expect("a parent that names nothing");

    let refused = import_claims_once(&Writer::default(), &pool, at())
        .await
        .expect_err("the import refuses");
    let ClaimsImportError::Findings { count, rows } = &refused else {
        panic!("a finding is named, not hidden: {refused:?}");
    };
    assert_eq!(*count, 1, "{rows}");
    assert!(rows.contains("person:moe"), "{rows}");
    assert!(logged(&pool).await.is_empty(), "nothing was appended");

    server.stop().await;
}

/// **A merge the store holds no number for imports as a redirect.** A merge made
/// before the merge-event table has no row there, and its folded entity keeps
/// `merged_into`. The import reads that mark: the folded entity answers to its
/// survivor, as it did, and the merge count does not move.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_reads_a_merge_with_no_event_as_a_redirect() {
    let (_scratch, mut server, pool) = store("claims-import-redirect").await;
    let memory = DoltMemory::open(pool.clone());
    let homer = thing(&memory, "person:homer", "Homer").await;
    let bart = thing(&memory, "person:bart", "Bart").await;
    let me = thing(&memory, "bot:sigma", "Sigma").await;
    memory
        .merge(
            &bart,
            &homer,
            Some("filed twice"),
            date(2026, 9, 2),
            &me,
            None,
        )
        .await
        .expect("the merge");
    sqlx::query("DELETE FROM merge_event")
        .execute(&pool)
        .await
        .expect("a store from before the merge-event table");

    let imported = import_claims_once(&Writer::default(), &pool, at())
        .await
        .expect("the import runs");
    assert!(matches!(imported, Imported::Done { .. }), "{imported:?}");
    let held = logged(&pool).await;
    let redirects: Vec<_> = held
        .iter()
        .filter(|event| matches!(event.payload, ClaimEvent::ObjectRedirected { .. }))
        .collect();
    assert_eq!(
        redirects.len(),
        1,
        "one folded entity, one redirect: {redirects:?}"
    );
    assert!(
        held.iter()
            .all(|event| !matches!(event.payload, ClaimEvent::ObjectsMerged { .. })),
        "no merge event row, so no numbered merge"
    );

    server.stop().await;
}

/// **A gap in the merge numbers stops the import with nothing appended.** Numbers
/// run 1, 2, 3. A merge whose rows are gone but whose folded entity still carries
/// its mark is no gap (the mark holds its number: the redirect case above). A gap
/// needs the rows gone AND the mark cleared, which only a hand repair does; the
/// mapping then refuses, and the import fails the call rather than number the
/// merges itself. Failing loud is the choice: a start that skipped the import
/// would serve a state that has no merges.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_refuses_a_gap_in_the_merge_numbers() {
    let (_scratch, mut server, pool) = store("claims-import-gap").await;
    let memory = DoltMemory::open(pool.clone());
    let homer = thing(&memory, "person:homer", "Homer").await;
    let bart = thing(&memory, "person:bart", "Bart").await;
    let lisa = thing(&memory, "person:lisa", "Lisa").await;
    let me = thing(&memory, "bot:sigma", "Sigma").await;
    for folded in [&bart, &lisa] {
        memory
            .merge(folded, &homer, None, date(2026, 9, 2), &me, None)
            .await
            .expect("a merge");
    }
    sqlx::query("DELETE FROM merge_event WHERE merge_no = 1")
        .execute(&pool)
        .await
        .expect("a store that lost the first merge's rows");
    sqlx::query("UPDATE entity SET merged_into = NULL WHERE id = 'person:bart'")
        .execute(&pool)
        .await
        .expect("a hand repair that cleared the folded entity's mark");

    let refused = import_claims_once(&Writer::default(), &pool, at())
        .await
        .expect_err("the import refuses");
    assert!(
        matches!(&refused, ClaimsImportError::Unmappable(why) if why.contains("without a gap")),
        "{refused:?}"
    );
    assert!(logged(&pool).await.is_empty(), "nothing was appended");

    server.stop().await;
}

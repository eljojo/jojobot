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
use jojobot_domain::memory::owned::{Provision, Provisions};
use jojobot_domain::memory::{
    Entity, EntityId, EntityKind, Memory, NewEntity, NewFact, Provenance,
};

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
    let first = import_claims_once(&writer, &pool, &Provisions::default(), at())
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

    let again = import_claims_once(&writer, &pool, &Provisions::default(), at())
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

    let refused = import_claims_once(&Writer::default(), &pool, &Provisions::default(), at())
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

    let imported = import_claims_once(&Writer::default(), &pool, &Provisions::default(), at())
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

    let refused = import_claims_once(&Writer::default(), &pool, &Provisions::default(), at())
        .await
        .expect_err("the import refuses");
    assert!(
        matches!(&refused, ClaimsImportError::Unmappable(why) if why.contains("without a gap")),
        "{refused:?}"
    );
    assert!(logged(&pool).await.is_empty(), "nothing was appended");

    server.stop().await;
}

/// A record the build supplies, as the boot hands it to the import.
fn supplied_view(handle: &str) -> Provisions {
    Provisions::new(vec![Provision::record(
        Entity {
            folded_names: Vec::new(),
            id: EntityId(handle.to_string()),
            kind: EntityKind::VIEW,
            name: "The loops view".into(),
            aliases: Vec::new(),
            source: "jojobot".into(),
            crm: None,
            parent: None,
            boot: Default::default(),
            merged_into: None,
            badge: None,
            archived: None,
        },
        std::collections::BTreeMap::new(),
    )])
}

/// **A claim filed under a handle the build supplies is placed at the record's
/// supply identity.** The overlay's claim has no row to name, so the reader used
/// to refuse it and the boot with it (rule 508: a supplied record never stops the
/// server). The import creates the record under its supply identity, binds its
/// handle, and files the claim there.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_files_a_claim_under_a_supplied_record_at_its_supply_identity() {
    let (_scratch, mut server, pool) = store("claims-import-supplied-home").await;
    let supplied = supplied_view("view:loops");
    let memory = DoltMemory::open(pool.clone()).knowing(supplied.clone());
    let loops = EntityId("view:loops".to_string());
    say(&memory, &loops, "the board is read on Mondays").await;

    let imported = import_claims_once(&Writer::default(), &pool, &supplied, at())
        .await
        .expect("a claim under a supplied record does not stop the import");
    assert!(matches!(imported, Imported::Done { .. }), "{imported:?}");
    let held = logged(&pool).await;
    let supply = jojobot_domain::log::claims::import::supply_identity("view:loops");
    assert!(
        held.iter().any(|event| matches!(
            &event.payload,
            ClaimEvent::ObjectCreated { object, .. } if *object == supply
        )),
        "the supplied record is created under its supply identity: {held:?}"
    );
    assert!(
        held.iter().any(|event| matches!(
            &event.payload,
            ClaimEvent::AddressBound { object, address }
                if *object == supply && address == "view:loops"
        )),
        "the supplied record answers to its handle: {held:?}"
    );
    assert!(
        held.iter().any(|event| matches!(
            &event.payload,
            ClaimEvent::RecordCreated { record, .. } if record.0.starts_with("supplied/view:loops/")
        )),
        "the claim is filed under the supply identity, not under the handle: {held:?}"
    );

    server.stop().await;
}

/// **A folded thing's claim keeps serving from the survivor after the import.**
/// Made with the real port, so the rows are the shape the store leaves: the
/// merge moves no row, the folded claim stays under the folded key with the
/// local id it had, and the merge row gives it the id it wears under the
/// survivor. The import is replayed into a State and the survivor is asked for
/// the key the folded thing's claim wrote.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_keeps_a_folded_things_claim_under_the_survivor() {
    let (_scratch, mut server, pool) = store("claims-import-merged-claim").await;
    let memory = DoltMemory::open(pool.clone());
    let bart = thing(&memory, "person:bart", "Bart").await;
    let lisa = thing(&memory, "person:lisa", "Lisa").await;
    let me = thing(&memory, "bot:sigma", "Sigma").await;
    // The survivor has a claim of its own, so the folded claim's local id is taken
    // under it and the merge gives the folded one another.
    let mut own = NewFact::about(bart.clone(), "skates", date(2026, 9, 1));
    own.provenance = Provenance::Testimony;
    own.fields = [("mood".to_string(), "mischievous".to_string())]
        .into_iter()
        .collect();
    own.sets = own.fields.keys().cloned().collect();
    memory
        .capture(own)
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
    let mut fact = NewFact::about(lisa.clone(), "plays the saxophone", date(2026, 9, 1));
    fact.provenance = Provenance::Testimony;
    fact.fields = [("instrument".to_string(), "saxophone".to_string())]
        .into_iter()
        .collect();
    fact.sets = fact.fields.keys().cloned().collect();
    memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
    memory
        .merge(
            &lisa,
            &bart,
            Some("filed twice"),
            date(2026, 9, 3),
            &me,
            Some("run-a"),
        )
        .await
        .expect("the fold lands");

    let read = memory.import_rows().await.expect("the reader reads");
    assert!(read.findings.is_empty(), "{:?}", read.findings);
    let events = import(&read.rows).expect("the rows map");
    let state = State::replay(events.iter());
    let survivor = read
        .rows
        .entities
        .iter()
        .find(|row| row.handle == "person:bart")
        .expect("the survivor is listed")
        .key
        .clone();
    assert_eq!(
        state
            .fields(&survivor, &[])
            .get("instrument")
            .map(String::as_str),
        Some("saxophone"),
        "the survivor lost the folded thing's key write"
    );

    server.stop().await;
}

/// **A claim that cites a folded thing's claim points at a record that exists.**
/// The store accepts the citation under the address the merge serves, the
/// survivor and the new label, and keeps it so. The record behind that address
/// was created under the folded key, so the import must name the record the
/// citation was created as, or the replayed state holds a pointer to nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_points_a_citation_at_the_record_a_merged_claim_was_created_as() {
    let (_scratch, mut server, pool) = store("claims-import-merged-citation").await;
    let memory = DoltMemory::open(pool.clone());
    let bart = thing(&memory, "person:bart", "Bart").await;
    let lisa = thing(&memory, "person:lisa", "Lisa").await;
    let me = thing(&memory, "bot:sigma", "Sigma").await;
    say(&memory, &bart, "skates").await;
    say(&memory, &lisa, "plays the saxophone").await;
    memory
        .merge(
            &lisa,
            &bart,
            Some("filed twice"),
            date(2026, 9, 3),
            &me,
            Some("run-a"),
        )
        .await
        .expect("the fold lands");
    // The folded claim is served under the survivor, after the survivor's own.
    let mut citing = NewFact::about(bart.clone(), "borrowed the saxophone", date(2026, 9, 4));
    citing.provenance = Provenance::Testimony;
    citing.derived_from = Some(jojobot_domain::memory::FactAddress::new(
        bart.clone(),
        jojobot_domain::memory::FactId("f2".to_string()),
    ));
    memory
        .capture(citing)
        .await
        .expect("capture")
        .written()
        .expect("the store accepts a citation at the address the merge serves");

    let read = memory.import_rows().await.expect("the reader reads");
    assert!(read.findings.is_empty(), "{:?}", read.findings);
    let state = State::replay(import(&read.rows).expect("the rows map").iter());
    let survivor = read
        .rows
        .entities
        .iter()
        .find(|row| row.handle == "person:bart")
        .expect("the survivor is listed")
        .key
        .clone();
    let cited = state
        .active_records(&survivor)
        .into_iter()
        .filter_map(|record| state.record(record))
        .find(|record| record.claim.content == "borrowed the saxophone")
        .expect("the citing claim is served from the survivor")
        .claim
        .derived_from
        .clone()
        .expect("the citation is kept");
    assert!(
        state.record(&cited).is_some(),
        "the citation names a record nothing created: {cited:?}"
    );

    server.stop().await;
}

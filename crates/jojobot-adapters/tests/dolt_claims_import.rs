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
    let mut events = import(&read.rows).expect("the rows map");
    // The import ends with the event that says it ran to its end.
    let mapped = events.len();
    events.push(jojobot_domain::log::claims::import::import_completed(
        mapped,
    ));
    events
        .iter()
        .map(|event| codec::decode(&codec::encode(event)).expect("an event reads back"))
        .collect()
}

/// The run's own day. `at()` is 08:00 UTC on the 15th of January 2027, so the day
/// before it is one no UTC reading of that clock gives.
fn the_day() -> jiff::civil::Date {
    date(2027, 1, 14)
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
    let Imported::Done { events, .. } = first else {
        panic!("the first call imports: {first:?}");
    };
    assert!(events > 0);

    let held = logged(&pool).await;
    assert_eq!(
        held.len(),
        events + 1,
        "the log holds what the call reports and the event that ends the import"
    );
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
        events + 1,
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

/// **A gap in the merge numbers is kept as the store holds it** (rule 677). A
/// merge whose rows are gone but whose folded entity still carries its mark is no
/// gap (the redirect case above). A gap needs the rows gone AND the mark cleared,
/// which a hand repair does, and a deployed store holds one. The import keeps the
/// stored numbers: the replayed State's highest merge number is the store's
/// highest, and the import names the empty number.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_keeps_a_gap_in_the_merge_numbers() {
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
    let highest: i64 = sqlx::query_scalar("SELECT MAX(merge_no) FROM merge_event")
        .fetch_one(&pool)
        .await
        .expect("the store's highest merge number");

    let imported = import_claims_once(&Writer::default(), &pool, &Provisions::default(), at())
        .await
        .expect("a gap does not stop the import");
    let Imported::Done {
        empty_merge_numbers,
        ..
    } = imported
    else {
        panic!("the import ran: {imported:?}");
    };
    assert_eq!(empty_merge_numbers, vec![1], "the empty number is named");
    let events: Vec<Event<ClaimEvent>> = logged(&pool).await;
    assert_eq!(
        State::replay(events.iter()).highest_merge_number(),
        highest as u64,
        "the State keeps the store's highest merge number"
    );

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

/// Several records the build supplies, as the boot hands them over.
fn supplied_views(handles: &[&str]) -> Provisions {
    Provisions::new(
        handles
            .iter()
            .map(|handle| {
                Provision::record(
                    Entity {
                        folded_names: Vec::new(),
                        id: EntityId((*handle).to_string()),
                        kind: EntityKind::VIEW,
                        name: "A view".into(),
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
                )
            })
            .collect(),
    )
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

/// **Claims under a name no build supplied are skipped and named** (the import
/// places what it can). The claim is made while the build supplies the record. The
/// import then runs with no such record in the supply list and no listing of it as
/// supplied: its rows were removed by hand, so nothing is adopted and nothing is
/// refused. The claims stay in the old tables and the report names the home with
/// its claim count; a thing beside it imports as usual.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_skips_and_names_the_claims_under_a_name_nothing_supplies() {
    let (_scratch, mut server, pool) = store("claims-import-unsupplied-home").await;
    let supplied = supplied_view("view:loops");
    let memory = DoltMemory::open(pool.clone()).knowing(supplied);
    let loops = EntityId("view:loops".to_string());
    say(&memory, &loops, "the board is read on Mondays").await;
    say(&memory, &loops, "and on Fridays").await;
    let homer = thing(&memory, "person:homer", "Homer").await;
    say(&memory, &homer, "likes donuts").await;

    let imported = import_claims_once(&Writer::default(), &pool, &Provisions::default(), at())
        .await
        .expect("a name nothing holds is skipped, not refused");
    let Imported::Done { skipped_homes, .. } = imported else {
        panic!("the import ran: {imported:?}");
    };
    assert_eq!(
        skipped_homes,
        vec![("view:loops".to_string(), 2)],
        "the home and its claim count are named"
    );
    let held = logged(&pool).await;
    assert!(
        held.iter().all(|event| !matches!(
            &event.payload,
            ClaimEvent::RecordCreated { record, .. } if record.0.contains("view:loops")
        )),
        "no claim of the skipped home entered the log: {held:?}"
    );
    assert!(
        held.iter().any(|event| matches!(
            &event.payload,
            ClaimEvent::RecordCreated { claim, .. } if claim.content == "likes donuts"
        )),
        "the claims of a placed thing imported: {held:?}"
    );

    server.stop().await;
}

/// **An edge and a reference that name a supplied record import at its supply
/// identity.** The record has no row, so the reader must place the name the claim
/// carries on the supplied record the build gives it, or the import refuses.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_places_an_edge_and_a_reference_on_a_supplied_record() {
    let (_scratch, mut server, pool) = store("claims-import-supplied-edge").await;
    let supplied = supplied_view("view:loops");
    let memory = DoltMemory::open(pool.clone()).knowing(supplied.clone());
    let bart = thing(&memory, "person:bart", "Bart").await;
    let loops = EntityId("view:loops".to_string());
    let mut fact = NewFact::about(bart.clone(), "reads the board", date(2026, 9, 1));
    fact.provenance = Provenance::Testimony;
    fact.edge = Some(jojobot_domain::memory::Edge::new(
        jojobot_domain::memory::EdgeShape::About,
        loops.clone(),
    ));
    fact.refs = vec![loops.clone()];
    memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("not blocked");

    let imported = import_claims_once(&Writer::default(), &pool, &supplied, at())
        .await
        .expect("an edge to a supplied record does not stop the import");
    assert!(matches!(imported, Imported::Done { .. }), "{imported:?}");
    let supply = jojobot_domain::log::claims::import::supply_identity("view:loops");
    let held = logged(&pool).await;
    let body = held
        .iter()
        .find_map(|event| match &event.payload {
            ClaimEvent::RecordCreated { claim, .. } if claim.content == "reads the board" => {
                Some(claim)
            }
            _ => None,
        })
        .expect("the claim is imported");
    assert_eq!(
        body.edge.as_ref().map(|(_, object)| object),
        Some(&supply),
        "the edge names the supply identity: {body:?}"
    );
    assert_eq!(body.refs, vec![supply], "the reference names it too");

    server.stop().await;
}

/// **A stored row at a supplied handle keeps the binding** (rule 508). The handle
/// is bound once, to the stored thing; the supplied record is created and never
/// takes the name from it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_leaves_a_stored_rows_handle_bound_to_the_stored_row() {
    let (_scratch, mut server, pool) = store("claims-import-supplied-collision").await;
    let memory = DoltMemory::open(pool.clone());
    thing(&memory, "view:loops", "A stored loops").await;

    let imported = import_claims_once(
        &Writer::default(),
        &pool,
        &supplied_view("view:loops"),
        at(),
    )
    .await
    .expect("a supplied handle a stored row wears does not stop the import");
    assert!(matches!(imported, Imported::Done { .. }), "{imported:?}");
    let supply = jojobot_domain::log::claims::import::supply_identity("view:loops");
    let bound: Vec<_> = logged(&pool)
        .await
        .into_iter()
        .filter_map(|event| match event.payload {
            ClaimEvent::AddressBound { object, address } if address == "view:loops" => Some(object),
            _ => None,
        })
        .collect();
    assert_eq!(bound.len(), 1, "the handle is bound once: {bound:?}");
    assert_ne!(bound[0], supply, "the stored row keeps the handle");

    server.stop().await;
}

/// **A retired handle carries the day the rename was made.** The rename writes
/// its own day to the former-handle row. The import reads it back from the real
/// store and puts it on the retirement, so a replay serves the same day.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_carries_the_day_of_a_rename_on_the_retirement() {
    let (_scratch, mut server, pool) = store("claims-import-retired-day").await;
    let memory = DoltMemory::open(pool.clone());
    let before = thing(&memory, "person:sigma", "Sigma").await;
    let after = EntityId("person:omega".to_string());
    memory
        .rename_entity(
            &before,
            &after,
            None,
            date(2026, 9, 4),
            jojobot_domain::memory::Override::new(None, None),
            None,
        )
        .await
        .expect("the rename answers")
        .written()
        .expect("the rename lands");

    let imported = import_claims_once(&Writer::default(), &pool, &Provisions::default(), at())
        .await
        .expect("the import runs");
    assert!(matches!(imported, Imported::Done { .. }), "{imported:?}");
    let days: Vec<_> = logged(&pool)
        .await
        .into_iter()
        .filter_map(|event| match event.payload {
            ClaimEvent::AddressRetired { address, day, .. } if address == "person:sigma" => {
                Some(day)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        days,
        vec![date(2026, 9, 4)],
        "the retirement carries the day"
    );

    server.stop().await;
}

/// **A handle the build stopped supplying is adopted by the import, and the
/// claims filed under it are placed on the adopted object.** The operator wrote
/// on a record the build supplied; a later build stopped supplying it and nothing
/// holds a row for it. The import adopts the handle once: the claim is filed
/// under the adopted identity, which the handle resolves to.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_adopts_a_handle_the_build_stopped_supplying() {
    let (_scratch, mut server, pool) = store("claims-import-adopt").await;
    let supplied = supplied_view("view:loops");
    let memory = DoltMemory::open(pool.clone()).knowing(supplied.clone());
    let loops = EntityId("view:loops".to_string());
    say(&memory, &loops, "the board is read on Mondays").await;
    // A boot lists what the build supplies, so a later build can tell it stopped.
    memory
        .record_supplied_handles(&[])
        .await
        .expect("the supplied handles are listed");

    // The next build supplies nothing at that handle.
    let imported = import_claims_once(&Writer::default(), &pool, &Provisions::default(), at())
        .await
        .expect("a retired supplied handle is adopted, not refused");
    assert!(matches!(imported, Imported::Done { .. }), "{imported:?}");
    let held = logged(&pool).await;
    let adopted = jojobot_domain::log::claims::import::adopted_identity("view:loops");
    assert!(
        held.iter().any(|event| matches!(
            &event.payload,
            ClaimEvent::ObjectCreated { object, .. } if *object == adopted
        )),
        "the handle is adopted under its own identity: {held:?}"
    );
    let state = State::replay(held.iter());
    assert_eq!(state.resolve("view:loops"), Some(&adopted));
    assert_eq!(
        state.active_records(&adopted).len(),
        1,
        "the claim sits on the adopted object"
    );

    server.stop().await;
}

/// **A later boot binds only what the supply list added, and an unchanged list
/// appends nothing.** The import made the record the first build supplied. A build
/// that supplies one more appends exactly that record's creation and binding in
/// one batch; the same list again appends no batch.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_reconcile_appends_the_new_record_and_then_nothing() {
    use jojobot_adapters::claims_import::reconcile_supply_list_once;
    let (_scratch, mut server, pool) = store("claims-reconcile").await;
    let first = supplied_views(&["view:loops"]);
    import_claims_once(&Writer::default(), &pool, &first, at())
        .await
        .expect("the import runs");
    let batches = |pool: sqlx::MySqlPool| async move {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM event_log WHERE first_position IS NOT NULL",
        )
        .fetch_one(&pool)
        .await
        .expect("the batches count")
    };
    let after_import = batches(pool.clone()).await;
    assert_eq!(
        reconcile_supply_list_once(&Writer::default(), &pool, &first, &[], at(), the_day())
            .await
            .expect("reconcile runs"),
        0,
        "the list the import saw appends nothing"
    );
    assert_eq!(batches(pool.clone()).await, after_import);

    let later = supplied_views(&["view:loops", "view:board"]);
    assert_eq!(
        reconcile_supply_list_once(&Writer::default(), &pool, &later, &[], at(), the_day())
            .await
            .expect("reconcile runs"),
        2,
        "exactly the new record's creation and binding"
    );
    assert_eq!(
        batches(pool.clone()).await,
        after_import + 1,
        "in one batch"
    );
    assert_eq!(
        reconcile_supply_list_once(&Writer::default(), &pool, &later, &[], at(), the_day())
            .await
            .expect("reconcile runs"),
        0,
        "the same list again appends nothing"
    );
    assert_eq!(batches(pool.clone()).await, after_import + 1);
    let state = State::replay(logged(&pool).await.iter());
    assert_eq!(
        state.resolve("view:board"),
        Some(&jojobot_domain::log::claims::import::supply_identity(
            "view:board"
        ))
    );

    server.stop().await;
}

/// **An import over an empty store still appends one member**, the event that says
/// it ran to its end, in a batch whose id begins `claims-import:`. A log that holds
/// that event is told from one that holds none, so a writer can refuse to emit
/// before the import is done. A second call finds the log holding it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_over_an_empty_store_appends_the_event_that_ends_it() {
    let (_scratch, mut server, pool) = store("claims-import-empty").await;
    let imported = import_claims_once(&Writer::default(), &pool, &Provisions::default(), at())
        .await
        .expect("an empty store imports");
    let Imported::Done { events, .. } = imported else {
        panic!("the import ran: {imported:?}");
    };
    assert_eq!(events, 0, "nothing to map");
    let held = logged(&pool).await;
    assert_eq!(held.len(), 1, "one member: {held:?}");
    assert!(
        matches!(held[0].payload, ClaimEvent::ImportCompleted { events: 0 }),
        "{held:?}"
    );
    let payloads: Vec<Vec<u8>> = sqlx::query_scalar(
        "SELECT payload FROM event_log WHERE first_position IS NOT NULL ORDER BY first_position",
    )
    .fetch_all(&pool)
    .await
    .expect("the batches read");
    assert!(
        payloads
            .iter()
            .all(|payload| String::from_utf8_lossy(payload).contains("claims-import:")),
        "the batch id begins claims-import:"
    );
    assert!(matches!(
        import_claims_once(&Writer::default(), &pool, &Provisions::default(), at())
            .await
            .expect("a second call"),
        Imported::AlreadyDone
    ));

    server.stop().await;
}

/// **Only an absent home is skipped; a handle two things once wore is refused by
/// name.** A claim filed under the text of a handle that more than one thing wore
/// has a home, and nothing may guess which: skipping it would drop a thing's data
/// as though its rows had been removed. The import names the ambiguity and appends
/// nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_refuses_a_claim_filed_under_a_handle_two_things_once_wore() {
    let (_scratch, mut server, pool) = store("claims-import-ambiguous-home").await;
    let memory = DoltMemory::open(pool.clone());
    thing(&memory, "person:homer", "Homer").await;
    thing(&memory, "person:marge", "Marge").await;
    let badges: Vec<String> =
        sqlx::query_scalar("SELECT badge FROM entity WHERE badge IS NOT NULL ORDER BY id")
            .fetch_all(&pool)
            .await
            .expect("the badges read");
    for (ordinal, badge) in badges.iter().take(2).enumerate() {
        sqlx::query(
            "INSERT INTO entity_former_handle (former_handle, badge, changed_at, ordinal)
             VALUES ('person:milhouse', ?, '2026-10-01', ?)",
        )
        .bind(badge)
        .bind(ordinal as i64 + 1)
        .execute(&pool)
        .await
        .expect("a former handle two things wore");
    }
    for statement in [
        "INSERT INTO fact (entity, id, content, details, provenance, standing, status,
                           recorded_at, edge_shape, edge_object, event_kind, derived_from,
                           derived_from_id, inserted_at, stale_after, happened_at,
                           happened_through)
         VALUES ('person:milhouse', 'f1', 'filed under the old text', NULL, 'testimony',
                 'settled', 'active', '2026-10-08', NULL, NULL, NULL, NULL, NULL,
                 '2026-10-08T14:02:10Z', NULL, NULL, NULL)",
        "INSERT INTO fact_write (entity, fact_id, ordinal, content, details, provenance,
                                standing, status, recorded_at, edge_shape, edge_object,
                                derived_from, derived_from_id, inserted_at, stale_after,
                                written_at, happened_at, happened_through, session)
         VALUES ('person:milhouse', 'f1', 1, 'filed under the old text', NULL, 'testimony',
                 'settled', 'active', '2026-10-08', NULL, NULL, NULL, NULL,
                 '2026-10-08T14:02:10Z', NULL, '2026-10-08T14:02:10Z', NULL, NULL, NULL)",
    ] {
        sqlx::query(statement)
            .execute(&pool)
            .await
            .expect("the claim lands");
    }

    let refused = import_claims_once(&Writer::default(), &pool, &Provisions::default(), at())
        .await
        .expect_err("an ambiguous home is refused, not skipped");
    let ClaimsImportError::Findings { rows, .. } = &refused else {
        panic!("the ambiguity is named: {refused:?}");
    };
    assert!(rows.contains("more than one thing"), "{rows}");
    assert!(logged(&pool).await.is_empty(), "nothing was appended");

    server.stop().await;
}

/// **A renamed record keeps its identity through a real store, and two records
/// cannot share a first handle.** The first build supplies `view:loops`; a later
/// one supplies the same record as `view:board`, naming its first handle. One
/// reconcile moves the address on the same identity. Two records naming one first
/// handle are refused before anything is written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_reconcile_moves_a_renamed_record_and_refuses_two_with_one_first_handle() {
    use jojobot_adapters::claims_import::reconcile_supply_list_once;
    let (_scratch, mut server, pool) = store("claims-reconcile-rename").await;
    import_claims_once(
        &Writer::default(),
        &pool,
        &supplied_views(&["view:loops"]),
        at(),
    )
    .await
    .expect("the import runs");

    let renamed = Provisions::new(
        supplied_views(&["view:board"])
            .records()
            .map(|(entity, fields)| {
                Provision::record(entity.clone(), fields.clone()).first_supplied_as("view:loops")
            })
            .collect(),
    );
    reconcile_supply_list_once(&Writer::default(), &pool, &renamed, &[], at(), the_day())
        .await
        .expect("the rename reconciles");
    let retired_on: Vec<_> = logged(&pool)
        .await
        .iter()
        .filter_map(|event| match &event.payload {
            ClaimEvent::AddressRetired { address, day, .. } if address == "view:loops" => {
                Some(*day)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        retired_on,
        vec![the_day()],
        "the rename retires the old spelling on the run's day, not on the clock's UTC day"
    );
    let state = State::replay(logged(&pool).await.iter());
    let identity = jojobot_domain::log::claims::import::supply_identity("view:loops");
    assert_eq!(state.resolve("view:board"), Some(&identity));
    assert_eq!(state.resolve("view:loops"), None);

    let twice = Provisions::new(
        ["view:board", "view:upgrade-fixture-view"]
            .iter()
            .flat_map(|handle| {
                supplied_views(&[handle])
                    .records()
                    .map(|(entity, fields)| {
                        Provision::record(entity.clone(), fields.clone())
                            .first_supplied_as("view:loops")
                    })
                    .collect::<Vec<_>>()
            })
            .collect(),
    );
    let refused =
        reconcile_supply_list_once(&Writer::default(), &pool, &twice, &[], at(), the_day())
            .await
            .expect_err("two records with one first handle are refused");
    assert!(
        matches!(&refused, ClaimsImportError::Unmappable(why) if why.contains("first supplied under")),
        "{refused:?}"
    );

    server.stop().await;
}

/// **A current name answers before a former one.** A build supplies `view:loops`
/// while a stored thing once wore that handle and left it. A claim filed under the
/// text belongs to the record the build supplies now, as `DoltMemory::resolve`
/// answers: badge, stored id, supplied handle, former handle, adoption. The claim
/// lands on the supply identity, not on the thing that wore the handle before.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_places_a_current_supplied_name_before_a_former_one() {
    let (_scratch, mut server, pool) = store("claims-import-current-before-former").await;
    let supplied = supplied_view("view:loops");
    let memory = DoltMemory::open(pool.clone()).knowing(supplied.clone());
    thing(&memory, "person:homer", "Homer").await;
    let badge: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = 'person:homer'")
        .fetch_one(&pool)
        .await
        .expect("the thing wears a badge");
    sqlx::query(
        "INSERT INTO entity_former_handle (former_handle, badge, changed_at, ordinal)
         VALUES ('view:loops', ?, '2026-10-01', 1)",
    )
    .bind(&badge)
    .execute(&pool)
    .await
    .expect("a former handle the thing wore");
    say(
        &memory,
        &EntityId("view:loops".to_string()),
        "the board is read on Mondays",
    )
    .await;

    import_claims_once(&Writer::default(), &pool, &supplied, at())
        .await
        .expect("the import runs");
    let held = logged(&pool).await;
    let supply = jojobot_domain::log::claims::import::supply_identity("view:loops");
    let state = State::replay(held.iter());
    assert_eq!(
        state.active_records(&supply).len(),
        1,
        "the claim sits on the record the build supplies now: {held:?}"
    );

    server.stop().await;
}

/// **A handle one thing left twice has one owner.** A thing wore `person:milhouse`,
/// left it, took it back and left it again, so its history holds the former handle
/// twice, both rows under one badge. A claim filed under that text belongs to that
/// one thing: the import places it there and finds nothing ambiguous. The case with
/// two different things stays refused (the case above).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_import_places_a_handle_one_thing_left_twice_on_that_thing() {
    let (_scratch, mut server, pool) = store("claims-import-left-twice").await;
    let memory = DoltMemory::open(pool.clone());
    thing(&memory, "person:homer", "Homer").await;
    let badge: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = 'person:homer'")
        .fetch_one(&pool)
        .await
        .expect("the thing wears a badge");
    for ordinal in 1..=2_i64 {
        sqlx::query(
            "INSERT INTO entity_former_handle (former_handle, badge, changed_at, ordinal)
             VALUES ('person:milhouse', ?, '2026-10-01', ?)",
        )
        .bind(&badge)
        .bind(ordinal)
        .execute(&pool)
        .await
        .expect("a former handle the one thing left twice");
    }
    for statement in [
        "INSERT INTO fact (entity, id, content, details, provenance, standing, status,
                           recorded_at, edge_shape, edge_object, event_kind, derived_from,
                           derived_from_id, inserted_at, stale_after, happened_at,
                           happened_through)
         VALUES ('person:milhouse', 'f1', 'filed under the old text', NULL, 'testimony',
                 'settled', 'active', '2026-10-08', NULL, NULL, NULL, NULL, NULL,
                 '2026-10-08T14:02:10Z', NULL, NULL, NULL)",
        "INSERT INTO fact_write (entity, fact_id, ordinal, content, details, provenance,
                                standing, status, recorded_at, edge_shape, edge_object,
                                derived_from, derived_from_id, inserted_at, stale_after,
                                written_at, happened_at, happened_through, session)
         VALUES ('person:milhouse', 'f1', 1, 'filed under the old text', NULL, 'testimony',
                 'settled', 'active', '2026-10-08', NULL, NULL, NULL, NULL,
                 '2026-10-08T14:02:10Z', NULL, '2026-10-08T14:02:10Z', NULL, NULL, NULL)",
    ] {
        sqlx::query(statement)
            .execute(&pool)
            .await
            .expect("the claim lands");
    }

    import_claims_once(&Writer::default(), &pool, &Provisions::default(), at())
        .await
        .expect("a handle one thing left twice is placed, not refused");
    let state = State::replay(logged(&pool).await.iter());
    assert_eq!(
        state
            .active_records(&jojobot_domain::log::ObjectId(badge))
            .len(),
        1,
        "the claim sits on the one thing that wore the handle"
    );

    server.stop().await;
}

/// **A release that changes a shipped kind reaches the State.** The import does
/// not declare kinds; the reconcile does, the first time and when a build changes
/// one. An earlier build's list declared `person` with another key. The build now
/// ships it as `seed` writes it today: one reconcile appends only that kind's new
/// declaration, the State's kind equals the build's, the import's marker stays
/// before it, and the same list appends nothing the second time.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_reconcile_declares_a_shipped_kind_the_build_changed() {
    use jojobot_adapters::claims_import::reconcile_supply_list_once;
    use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
    let (_scratch, mut server, pool) = store("claims-reconcile-kinds").await;
    import_claims_once(&Writer::default(), &pool, &Provisions::default(), at())
        .await
        .expect("the import runs");
    let build = jojobot_domain::memory::kinds::shipped_declarations();
    // The list an earlier build declared: the same kinds, `person` with another key.
    let earlier: Vec<DeclaredType> = build
        .iter()
        .map(|declaration| {
            if declaration.name == "person" {
                DeclaredType::shipped("person", vec![Field::new("an_old_key", ValueType::Text)])
            } else {
                declaration.clone()
            }
        })
        .collect();
    let run = |list: Vec<DeclaredType>| {
        let pool = pool.clone();
        async move {
            reconcile_supply_list_once(
                &Writer::default(),
                &pool,
                &Provisions::default(),
                &list,
                at(),
                the_day(),
            )
            .await
            .expect("reconcile runs")
        }
    };
    assert_eq!(
        run(earlier.clone()).await,
        build.len(),
        "the first boot declares every shipped kind"
    );
    assert_eq!(run(earlier).await, 0, "the same list appends nothing");
    assert_eq!(
        run(build.clone()).await,
        1,
        "only the kind the build changed is declared again"
    );
    let held = logged(&pool).await;
    let state = State::replay(held.iter());
    let person = build
        .iter()
        .find(|declaration| declaration.name == "person")
        .expect("person is shipped");
    assert_eq!(
        state.declared(&jojobot_domain::log::ObjectId("person".into())),
        Some(person),
        "the State's kind is the build's"
    );
    let ended = held
        .iter()
        .position(|event| matches!(event.payload, ClaimEvent::ImportCompleted { .. }))
        .expect("the import ended");
    let declared = held
        .iter()
        .rposition(
            |event| matches!(&event.payload, ClaimEvent::TypeDeclared { kind, .. } if kind.0 == "person"),
        )
        .expect("the kind is declared");
    assert!(
        ended < declared,
        "the import's marker stays before the new declaration"
    );
    assert_eq!(
        run(build).await,
        0,
        "the build's list appends nothing again"
    );

    server.stop().await;
}

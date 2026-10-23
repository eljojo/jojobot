//! **What a capture costs for each claim its subject holds and for each thing
//! that shares a name with it.**
//!
//! After a claim write the fold used to ask the store for the claims of every
//! thing that named the same targets as the subject, so a capture on a thing whose
//! claims name an owner cost the claims of every thing with that owner. The fold
//! now patches its lists from the subject's own documents.
//!
//! Two cases. The first counts the statements of a capture through the fold, on a
//! store of 800 other things that name one owner, for a thing of no claims and
//! for a thing of 48. It prints the table with `--nocapture`. The second holds
//! the patched lists to a full ask of the store after each kind of claim write.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::fold::Folded;
use jojobot_adapters::provisioned::Provisioned;
use jojobot_adapters::search::IndexedMemory;
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::memory::mention::Mentioning;
use jojobot_domain::memory::owned::Provisions;
use jojobot_domain::memory::testing::contract as memory;
use jojobot_domain::memory::{
    Edge, EdgeShape, EntityId, Fact, FactPatch, FactStatus, Guarded, Memory, NewEntity, NewFact,
    Provenance, guard,
};

const CLAIMS: usize = 48;
/// Other things the store holds, each with a few claims: a production store holds
/// about a thousand, and a cost that follows the store needs them to show.
const BACKGROUND: usize = 800;
/// How many more statements a capture on the thing of 48 claims may send than a
/// capture on the thing of none. The thing's own document is read once, which
/// takes a few more statements for more claims.
const MORE_STATEMENTS_PER_THING_OF_48: i64 = 30;
/// The most statements one capture through the fold may send, whoever else names
/// the owner it names. A capture is the insert and a read of its own thing: about
/// 130 statements. Reading the claims of every other thing with that owner sent
/// more than ten thousand.
const MOST_STATEMENTS_FOR_ONE_CAPTURE: i64 = 250;

/// A claim body of the length the build bots' big items carry.
fn body(n: usize, version: usize) -> String {
    let sentence = format!(
        "Slice {n} version {version} states what is built, why it is shaped so, and what it leaves out. "
    );
    sentence.repeat(25)
}

/// A directory of this run's own, removed when it is done.
struct Scratch(PathBuf);

impl Scratch {
    fn new(what: &str) -> Self {
        let path = jojobot_adapters::testing::scratch_path(&format!("jojobot-contract-{what}"));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Scratch(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The statements the server has executed so far, from its own counter.
async fn statements_run(pool: &sqlx::MySqlPool) -> i64 {
    use sqlx::Row;
    let row = sqlx::query("SHOW GLOBAL STATUS LIKE 'Questions'")
        .fetch_one(pool)
        .await
        .expect("the server counts its statements");
    row.try_get::<String, _>("Value")
        .expect("the counter has a value")
        .parse()
        .expect("the counter is a number")
}

/// The statements and the milliseconds a write took, the counter's own question
/// excluded.
async fn measured(
    pool: &sqlx::MySqlPool,
    work: impl std::future::Future<Output = Fact>,
) -> (i64, u128) {
    let before = statements_run(pool).await;
    let began = Instant::now();
    work.await;
    let millis = began.elapsed().as_millis();
    (statements_run(pool).await - before - 1, millis)
}

/// A store with the kinds seeded, behind the pool the counter reads.
async fn store_of(store: &Dolt, name: &str) -> (DoltMemory, sqlx::MySqlPool) {
    let pool = migrated_database(store, name)
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    (DoltMemory::open(pool.clone()), pool)
}

/// Add a thing, overriding the near-miss screen when things of one family share
/// words.
async fn add(memory: &dyn Memory, new: NewEntity) {
    match memory.add_entity(new.clone()).await.expect("add") {
        Guarded::Blocked {
            attempted,
            candidates,
        } => {
            memory
                .add_entity(NewEntity {
                    override_token: Some(guard::override_token(&attempted, &candidates)),
                    override_reason: Some("a fixture of the same family".into()),
                    ..new
                })
                .await
                .expect("add again")
                .written()
                .expect("not blocked twice");
        }
        other => {
            other.written().expect("written");
        }
    }
}

/// The bot every write is made as.
fn caller() -> EntityId {
    EntityId("bot:sigma".into())
}

/// A capture that must land.
async fn put(memory: &dyn Memory, fact: NewFact) -> Fact {
    let landed = memory
        .capture(memory::state(fact))
        .await
        .expect("the capture lands");
    let said = format!("{landed:?}");
    landed
        .written()
        .unwrap_or_else(|| panic!("blocked: {}", said.chars().take(400).collect::<String>()))
}

/// A claim of `thing` naming `owner`, as the build bots' work items do.
fn claim(thing: &EntityId, owner: &EntityId, n: usize, anchor: Option<&EntityId>) -> NewFact {
    NewFact {
        details: Some(body(n, 0)),
        edge: anchor.filter(|_| n.is_multiple_of(4)).map(|anchor| Edge {
            shape: EdgeShape::Connection,
            object: anchor.clone(),
        }),
        fields: [
            (format!("key_{}", n % 6), format!("value {n}")),
            ("status".to_string(), "now".to_string()),
            ("owner".to_string(), owner.as_str().to_string()),
        ]
        .into_iter()
        .collect(),
        ..NewFact::about(thing.clone(), body(n, 0), date(2026, 10, 1))
    }
}

/// **A capture through the fold sends a bounded number of statements, and a few
/// more for a thing of 48 claims than for a thing of none, however many other
/// things name the same owner.**
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_capture_through_the_fold_costs_no_more_for_a_thing_of_48_claims() {
    let scratch = Scratch::new("write-cost");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let (memory_store, counter) = store_of(&store, "writecost").await;
    let owner = EntityId("bot:assistant".into());
    let anchor = EntityId("topic:write-cost-anchor".into());
    let empty = EntityId("work:write-cost-empty".into());
    let wide = EntityId("work:write-cost-wide".into());
    for (id, name) in [
        (&owner, "Assistant"),
        (&anchor, "Cost anchor"),
        (&empty, "Bare notebook"),
        (&wide, "Wide ledger"),
    ] {
        add(
            &memory_store,
            NewEntity::new(id.clone(), name, "cost-fixture"),
        )
        .await;
    }
    for n in 0..BACKGROUND {
        let id = EntityId(format!("work:write-cost-bg-{n}"));
        add(
            &memory_store,
            NewEntity::new(
                id.clone(),
                format!("Background piece {n} of the cost fixture"),
                "cost-fixture",
            ),
        )
        .await;
        for c in 0..3 {
            put(&memory_store, claim(&id, &owner, c, None)).await;
        }
    }
    for n in 0..CLAIMS {
        put(&memory_store, claim(&wide, &owner, n, Some(&anchor))).await;
    }

    // The stack the server serves through, store to index.
    let supplied = Provisions::new(Vec::new());
    let provisioned: Arc<dyn Memory> = Arc::new(Provisioned::new(
        DoltMemory::open(counter.clone()).knowing(supplied.clone()),
        supplied,
    ));
    let folded = Arc::new(Folded::new(provisioned));
    folded.rebuild().await.expect("the image fills");
    let indexed = Arc::new(
        IndexedMemory::new(Arc::new(Mentioning::new(folded.clone())))
            .expect("the search index opens"),
    );
    indexed.rebuild().await.expect("the boot scan reads");

    let mut counted = Vec::new();
    for (label, thing) in [("0 claims", &empty), ("48 claims", &wide)] {
        for (layer_name, layer) in [
            ("fold", folded.clone() as Arc<dyn Memory>),
            ("fold+mention+index", indexed.clone() as Arc<dyn Memory>),
        ] {
            let (statements, millis) = measured(
                &counter,
                put(layer.as_ref(), claim(thing, &owner, 1000, None)),
            )
            .await;
            println!("{label:<10} via {layer_name:<20} {statements:>6} statements {millis:>7} ms");
            if layer_name == "fold" {
                counted.push(statements);
            }
        }
    }
    let (of_none, of_48) = (counted[0], counted[1]);
    assert!(
        of_none > 0,
        "the capture sent nothing, so the count measures nothing: {of_none}"
    );
    // **The cost must not follow the other things that name the owner.** Both
    // subjects name it, so the difference between them says nothing about that:
    // the bound on one capture does.
    for (label, sent) in [("none", of_none), ("48", of_48)] {
        assert!(
            sent <= MOST_STATEMENTS_FOR_ONE_CAPTURE,
            "a capture on a thing of {label} claims sent {sent} statements, with {BACKGROUND} \
             other things naming the same owner"
        );
    }
    assert!(
        of_48 <= of_none + MORE_STATEMENTS_PER_THING_OF_48,
        "a capture on a thing of 48 claims sent {of_48} statements against {of_none} on a thing \
         of none, with {BACKGROUND} other things naming the same owner"
    );
    store.stop().await;
}

/// **The fold's list of who points at a name equals a full ask of the store after
/// each kind of claim write on a subject that names it.** The writes: a capture,
/// an edit that moves the claim off the name, an edit that puts it back, an archive,
/// and a retraction. The list is compared as the set of records it holds, because
/// the store does not order its answer.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_fold_lists_who_points_at_a_name_as_a_full_ask_does_after_each_claim_write() {
    let scratch = Scratch::new("write-cost-differential");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let (bare, counter) = store_of(&store, "writecostdiff").await;
    let owner = EntityId("bot:assistant".into());
    let other = EntityId("bot:delta".into());
    let subject = EntityId("work:write-cost-subject".into());
    let sibling = EntityId("work:write-cost-sibling".into());
    for (id, name) in [
        (&owner, "Assistant"),
        (&other, "Delta"),
        (&subject, "Subject ledger"),
        (&sibling, "Sibling ledger"),
    ] {
        add(&bare, NewEntity::new(id.clone(), name, "cost-fixture")).await;
    }
    // Another thing names the owner too, so a patch that dropped the other
    // holders would show.
    put(&bare, claim(&sibling, &owner, 0, None)).await;
    let first = put(&bare, claim(&subject, &owner, 1, None)).await;

    let supplied = Provisions::new(Vec::new());
    let provisioned: Arc<dyn Memory> = Arc::new(Provisioned::new(
        DoltMemory::open(counter.clone()).knowing(supplied.clone()),
        supplied,
    ));
    let folded = Arc::new(Folded::new(provisioned.clone()));
    folded.rebuild().await.expect("the image fills");

    let held = async |when: &str| {
        for name in [&owner, &other] {
            let mut patched = folded.referring_to(name).await.expect("the fold answers");
            let mut asked = provisioned
                .referring_to(name)
                .await
                .expect("the store answers");
            let order = |a: &Fact, b: &Fact| {
                (a.home.as_str(), a.id.as_str()).cmp(&(b.home.as_str(), b.id.as_str()))
            };
            patched.sort_by(order);
            asked.sort_by(order);
            assert_eq!(
                format!("{patched:?}"),
                format!("{asked:?}"),
                "{when}: who points at {name} differs between the fold and a full ask"
            );
        }
    };
    held("after the fill").await;

    let second = put(folded.as_ref(), claim(&subject, &owner, 2, None)).await;
    held("after a capture").await;

    // The claim stops naming the owner and names the other bot.
    let moved = folded
        .update_fact(
            &second.address(),
            memory::state_patch(FactPatch {
                fields: [("owner".to_string(), other.as_str().to_string())]
                    .into_iter()
                    .collect(),
                provenance: Some(Provenance::Inference),
                ..Default::default()
            }),
            &caller(),
        )
        .await
        .expect("the edit lands");
    assert!(moved.written().is_some(), "the edit was blocked");
    held("after an edit that moves the claim to another name").await;

    folded
        .update_fact(
            &second.address(),
            memory::state_patch(FactPatch {
                fields: [("owner".to_string(), owner.as_str().to_string())]
                    .into_iter()
                    .collect(),
                provenance: Some(Provenance::Inference),
                ..Default::default()
            }),
            &caller(),
        )
        .await
        .expect("the edit lands")
        .written()
        .expect("not blocked");
    held("after an edit that puts it back").await;

    folded
        .update_fact(
            &first.address(),
            FactPatch {
                status: Some(FactStatus::Archived),
                ..Default::default()
            },
            &caller(),
        )
        .await
        .expect("the archive lands")
        .written()
        .expect("not blocked");
    held("after an archive").await;

    folded
        .retract(
            &second.address(),
            Some("a fixture taken back"),
            date(2026, 10, 2),
            &caller(),
            None,
        )
        .await
        .expect("the retraction lands");
    held("after a retraction").await;

    store.stop().await;
}

//! **What a search costs the real store after a write.**
//!
//! A write re-reads the one document it touched, and the index is then current
//! with the store. A search that follows must not read the whole store to learn
//! that, and a change made past the application must still be found. The
//! instrument is the store's own statement log, counted per search.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::memory::search::{Hit, Search, SearchQuery};
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};
use tracing_subscriber::layer::{Context, SubscriberExt};

static STATEMENTS: AtomicUsize = AtomicUsize::new(0);

/// **The one statement the driver sends when it opens a connection**, which it
/// logs beside the statements of the code that asked for the connection. It is
/// the pool's and not the code's: the pool opens a connection when none is idle.
const SESSION_SETUP: &str = "SET sql_mode";

/// Reads the `summary` of a logged statement.
struct Summary(String);

impl tracing::field::Visit for Summary {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "summary" {
            self.0 = format!("{value:?}").trim_matches('"').to_string();
        }
    }
}

/// Counts the statements the store's driver logs, one event each, **except the
/// session setup of a connection the pool opened**.
struct CountStatements;

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CountStatements {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        if event.metadata().target() != "sqlx::query" {
            return;
        }
        let mut summary = Summary(String::new());
        event.record(&mut summary);
        if !summary.0.starts_with(SESSION_SETUP) {
            STATEMENTS.fetch_add(1, Ordering::SeqCst);
        }
    }
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// How many statements `run` cost.
async fn counted<T>(run: impl std::future::Future<Output = T>) -> (T, usize) {
    let before = STATEMENTS.load(Ordering::SeqCst);
    let out = run.await;
    (out, STATEMENTS.load(Ordering::SeqCst) - before)
}

fn names(hits: &[Hit], who: &EntityId) -> bool {
    hits.iter().any(|hit| match hit {
        Hit::Entity { entity, .. } => &entity.id == who,
        Hit::Fact { subject, .. } => &subject.id == who,
        _ => false,
    })
}

/// **A connection the pool opens inside the window is not a statement of the
/// code under count.** The pool opens a connection when none is idle, and the
/// one the previous call released is returned to the pool by a task that a loaded
/// machine runs late. The new connection's session setup is logged beside the
/// statements, and counting it made a search that opened one cost one more than
/// the search before it. The case opens a connection inside the window on
/// purpose and asks for the one statement it sent.
#[tokio::test]
async fn opening_a_connection_inside_the_window_adds_nothing_to_the_count() {
    let scratch = Scratch(jojobot_adapters::testing::scratch_path(
        "jojobot-after-a-write-count",
    ));
    std::fs::create_dir_all(&scratch.0).expect("a scratch directory");
    let port = free_port();
    let mut store = Dolt::start(&scratch.0, port)
        .await
        .expect("the store comes up");
    let _counting =
        tracing::subscriber::set_default(tracing_subscriber::registry().with(CountStatements));

    // A pool of its own has no connection yet, so its first statement opens one.
    let fresh = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(1)
        .connect_lazy(&format!("mysql://root@127.0.0.1:{port}/jojobot"))
        .expect("a pool of its own");
    let (answer, statements) = counted(async {
        sqlx::query_scalar::<_, i64>("SELECT 1")
            .fetch_one(&fresh)
            .await
    })
    .await;
    assert_eq!(answer.expect("the statement runs"), 1);
    assert_eq!(
        statements, 1,
        "the statement is counted and the connection it opened is not"
    );
    store.stop().await;
}

#[tokio::test]
async fn a_search_after_a_write_costs_what_a_search_after_none_costs() {
    let scratch = Scratch(jojobot_adapters::testing::scratch_path(
        "jojobot-after-a-write",
    ));
    std::fs::create_dir_all(&scratch.0).expect("a scratch directory");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "afterawrite")
        .await
        .expect("a database of its own");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");

    // The sink the store installs is global and quiet; the count is scoped to
    // this thread, which is the one the whole case runs on.
    let _counting =
        tracing::subscriber::set_default(tracing_subscriber::registry().with(CountStatements));

    let indexed = Arc::new(
        IndexedMemory::new(Arc::new(DoltMemory::open(pool.clone())))
            .expect("the search index opens"),
    );
    indexed.rebuild().await.expect("the boot scan reads");
    let retrieval = Retrieval::new(indexed.index(), vec![indexed.clone()]);

    let bart = EntityId::person("person:bart");
    let milhouse = EntityId::person("person:milhouse");
    for (id, name) in [(&bart, "Bart"), (&milhouse, "Milhouse")] {
        indexed
            .add_entity(NewEntity::new(id.clone(), name, "contract-fixture"))
            .await
            .expect("add_entity ok")
            .written()
            .expect("nothing collides with it");
    }

    // One search to settle, then one with nothing written between: the price of
    // a search that finds the store unchanged.
    retrieval
        .search(&SearchQuery::text("Bart"))
        .await
        .expect("search ok");
    let (idle_hits, idle) = counted(retrieval.search(&SearchQuery::text("Bart"))).await;
    assert!(names(&idle_hits.expect("search ok"), &bart));
    assert!(idle > 0, "the instrument counts statements: {idle}");

    // A write, then the search that follows it.
    indexed
        .capture(NewFact::about(
            bart.clone(),
            "keeps a lasagna in the cellar",
            date(2026, 7, 1),
        ))
        .await
        .expect("capture ok")
        .written()
        .expect("nothing collides with it");
    let (after_hits, after_a_write) =
        counted(retrieval.search(&SearchQuery::text("lasagna"))).await;
    assert!(
        names(&after_hits.expect("search ok"), &bart),
        "the claim just written is found"
    );
    assert_eq!(
        after_a_write, idle,
        "a search after a write costs what a search after none costs"
    );

    // A change made past the application: the same store, edited by hand.
    let badge: Option<String> = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(milhouse.as_str())
        .fetch_one(&pool)
        .await
        .expect("the row is readable");
    sqlx::query("UPDATE entity SET name = ? WHERE id = ?")
        .bind("Renamed By Hand")
        .bind(milhouse.as_str())
        .execute(&pool)
        .await
        .expect("the direct edit lands");
    assert!(badge.is_some(), "a created entity is given a badge");
    let (hand_hits, after_a_hand_edit) =
        counted(retrieval.search(&SearchQuery::text("Renamed By Hand"))).await;
    assert!(
        names(&hand_hits.expect("search ok"), &milhouse),
        "an edit made past the application is found"
    );
    assert!(
        after_a_hand_edit > idle,
        "the search that finds a hand edit reads the store: {after_a_hand_edit} vs {idle}"
    );

    store.stop().await;
}

//! **A bot that wrote a live record of a thread is one of its writers, in the
//! lineage a merge is judged by**, in the real store (rule 598).
//!
//! The runs are rows in the store's own session table, which the shared
//! contract cannot make, so this case stands beside the contract and the
//! in-memory store's case (`jojobot-domain`, `thread_writers.rs`).

use std::path::PathBuf;

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::sessions::DoltSessions;
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::memory::{Edge, EdgeShape, EntityId, Memory, NewEntity, NewFact};
use jojobot_domain::session::testing::contract as sessions;
use jojobot_domain::session::{NewSession, Sessions, Sid};

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

async fn booted(pool: &sqlx::MySqlPool) {
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
}

/// **What a handle's row is stored under**, for a session row that names the bot
/// by the key its entity is filed under.
async fn badge_of(pool: &sqlx::MySqlPool, handle: &str) -> String {
    sqlx::query_scalar::<_, Option<String>>("SELECT badge FROM entity WHERE id = ?")
        .bind(handle)
        .fetch_one(pool)
        .await
        .expect("the entity row is readable")
        .expect("a created entity is given a badge")
}

/// **lisa wrote a note on one duplicate and not on the other**: the merge refusal
/// says a clear of the ceiling opens for the second and not for the first, and a
/// bot that set a ceiling and took it off again is not made a writer by it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dolt_counts_the_bots_that_wrote_a_record_among_the_writers_a_merge_is_judged_by() {
    use jojobot_domain::memory::MemoryError;
    let scratch = Scratch::new("record-writers");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "recordwriters")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone());
    let sessions = DoltSessions::open(pool.clone());
    for (handle, name) in [
        ("bot:milhouse", "Milhouse"),
        ("bot:lisa", "Lisa"),
        ("thread:milhouse-moves", "Milhouse moves"),
        ("thread:simpson-kitchen", "Simpson kitchen"),
        ("thread:krusty-telethon", "Krusty telethon"),
        ("thread:alpha", "Alpha"),
        ("thread:beta", "Beta"),
    ] {
        memory
            .add_entity(NewEntity::new(EntityId(handle.into()), name, "case"))
            .await
            .expect("added")
            .written()
            .expect("not blocked");
    }
    let milhouse = EntityId("bot:milhouse".into());
    let lisa = EntityId("bot:lisa".into());
    let survivor = EntityId("thread:milhouse-moves".into());
    let lisa_key = EntityId(badge_of(&pool, "bot:lisa").await);
    for (bot, run) in [(milhouse.clone(), "sa01"), (lisa_key, "sb01")] {
        sessions
            .begin(NewSession {
                bot,
                sid: Sid(run.into()),
                focus: "writing".into(),
                started_at: sessions::epoch(),
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");
    }
    // milhouse keeps a thought in the survivor, so milhouse is its writer and
    // lisa, who is not above milhouse, may not carry a ceiling onto it.
    memory
        .capture(NewFact {
            edge: Some(Edge::new(EdgeShape::Connection, survivor.clone())),
            session: Some("sa01".into()),
            ..NewFact::about(milhouse.clone(), "milhouse follows it", date(2026, 10, 1))
        })
        .await
        .expect("captured")
        .written()
        .expect("not blocked");
    for duplicate in ["thread:simpson-kitchen", "thread:krusty-telethon"] {
        memory
            .capture(NewFact {
                sets: ["record_capacity".to_string()].into_iter().collect(),
                fields: [("record_capacity".to_string(), "5".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(
                    EntityId(duplicate.into()),
                    "how many notes it holds",
                    date(2026, 10, 1),
                )
            })
            .await
            .expect("captured")
            .written()
            .expect("not blocked");
    }
    // lisa writes a plain note on the first duplicate only.
    memory
        .capture(NewFact {
            session: Some("sb01".into()),
            ..NewFact::about(
                EntityId("thread:simpson-kitchen".into()),
                "a note from lisa",
                date(2026, 10, 1),
            )
        })
        .await
        .expect("captured")
        .written()
        .expect("not blocked");

    let clear_opens = |duplicate: &'static str| {
        let (memory, lisa, survivor) = (&memory, lisa.clone(), survivor.clone());
        async move {
            match memory
                .merge(
                    &EntityId(duplicate.into()),
                    &survivor,
                    None,
                    date(2026, 10, 2),
                    &lisa,
                    None,
                )
                .await
            {
                Err(MemoryError::MergeCarriesGuardedKeys { clear_opens, .. }) => clear_opens,
                other => panic!("the merge was expected to be refused: {:?}", other.is_ok()),
            }
        }
    };
    assert!(
        !clear_opens("thread:simpson-kitchen").await,
        "lisa wrote a note there, so the clear is refused to lisa and is not named"
    );
    assert!(
        clear_opens("thread:krusty-telethon").await,
        "lisa wrote nothing there, so the clear opens"
    );

    // ── lisa set a ceiling on another thread through a record and took it off ──
    // That record is the setting of the ceiling, once or now, and not a fill, so
    // lisa is no writer there and may carry a ceiling onto it.
    let alpha = EntityId("thread:alpha".into());
    let setting = memory
        .capture(NewFact {
            sets: ["thought_capacity".to_string()].into_iter().collect(),
            fields: [("thought_capacity".to_string(), "3".to_string())]
                .into_iter()
                .collect(),
            session: Some("sb01".into()),
            ..NewFact::about(
                alpha.clone(),
                "how many thoughts it holds",
                date(2026, 10, 3),
            )
        })
        .await
        .expect("captured")
        .written()
        .expect("not blocked");
    memory
        .update_fact(
            &setting.address(),
            jojobot_domain::memory::FactPatch {
                clear_fields: vec!["thought_capacity".to_string()],
                ..Default::default()
            },
            &lisa,
        )
        .await
        .expect("the ceiling is taken off")
        .written()
        .expect("not blocked");
    let beta = EntityId("thread:beta".into());
    memory
        .capture(NewFact {
            sets: ["record_capacity".to_string()].into_iter().collect(),
            fields: [("record_capacity".to_string(), "5".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(beta.clone(), "how many notes it holds", date(2026, 10, 3))
        })
        .await
        .expect("captured")
        .written()
        .expect("not blocked");
    memory
        .merge(&beta, &alpha, None, date(2026, 10, 4), &lisa, None)
        .await
        .expect("lisa wrote no record of her own on alpha, so she may carry a ceiling onto it");
    store.stop().await;
}

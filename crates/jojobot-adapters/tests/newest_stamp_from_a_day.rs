//! **The newest stamp the store holds from a day on, read across every table
//! that keeps one.**
//!
//! A stated clock continues from this at boot, so the answer has to be the
//! newest of all of them and not of the first table looked at, and a stamp on a
//! later day is newer than any on the day. Each case writes through the port
//! that owns a table, then asks the store.

use jiff::Timestamp;
use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::sessions::DoltSessions;
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::memory::EntityId;
use jojobot_domain::session::{NewEntry, NewSession, Sessions, Sid};

/// A directory of this run's own, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn at(raw: &str) -> Timestamp {
    raw.parse().expect("a moment")
}

fn run(sid: &str, started_at: Timestamp) -> NewSession {
    NewSession {
        bot: EntityId("bot:marge".into()),
        sid: Sid(sid.into()),
        focus: "working".into(),
        started_at,
        timezone: None,
        started_on: None,
        id: None,
        key_hash: None,
    }
}

/// **The newest stamp on the day is found, wherever it sits.** The run began at
/// ten, the entry on it was taken in at eleven, and a second run began at nine:
/// the answer is the entry's stamp, which is in a different table from the
/// other two. A stamp the day before, and a row the migration ledger wrote on
/// the day, are not the answer.
#[tokio::test]
async fn the_newest_stamp_from_a_day_is_read_across_tables_and_leaves_out_the_day_before_the_ledger_and_the_pass_record()
 {
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "jojobot-newest-stamp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock after 1970")
            .as_nanos()
    )));
    std::fs::create_dir_all(&scratch.0).expect("a scratch directory");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "newest_stamp")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let sessions = DoltSessions::open(pool.clone());

    let first = sessions
        .begin(run("nss1", at("2026-06-01T10:00:00Z")))
        .await
        .expect("begin ok");
    sessions
        .begin(run("nss2", at("2026-06-01T09:00:00Z")))
        .await
        .expect("begin ok");
    // A fraction is the only thing that tells these two apart as text:
    // `…:00.5Z` sorts before `…:00Z`, and it is the later of the two.
    sessions
        .append(
            &first.id,
            NewEntry {
                text: "found the door".into(),
                at: at("2026-06-01T11:00:00.5Z"),
                beat: None,
                on: None,
                closing_focus: None,
                closing: false,
            },
        )
        .await
        .expect("append ok");
    sessions
        .append(
            &first.id,
            NewEntry {
                text: "opened the box".into(),
                at: at("2026-06-01T11:00:00Z"),
                beat: None,
                on: None,
                closing_focus: None,
                closing: false,
            },
        )
        .await
        .expect("append ok");
    sessions
        .begin(run("nss3", at("2026-05-31T23:30:00Z")))
        .await
        .expect("begin ok");
    sqlx::query(
        "INSERT INTO schema_migration (version, applied_at) VALUES ('0000_ledger', '2026-06-01T23:59:59Z')",
    )
    .execute(&pool)
    .await
    .expect("a ledger row");
    sqlx::query(
        "INSERT INTO store_pass (name, finished_at) VALUES ('probe_pass', '2026-06-01T23:59:58Z')",
    )
    .execute(&pool)
    .await
    .expect("a pass record row");

    let newest = jojobot_adapters::dolt::newest_stamp_from(&pool, date(2026, 6, 1))
        .await
        .expect("the store answers");

    store.stop().await;
    assert_eq!(newest, Some(at("2026-06-01T11:00:00.5Z")));
}

/// **A stamp on a later day is the newest.** The store holds a run on the
/// stated day and one begun the day after it, and the clock that continues from
/// the answer must stamp after both. The run on the day alone is the control: it
/// is the answer when nothing is later.
#[tokio::test]
async fn a_stamp_on_a_later_day_is_the_newest_and_a_store_with_only_the_day_answers_with_the_day() {
    let scratch = Scratch(
        std::env::temp_dir().join(format!("jojobot-newest-stamp-later-{}", std::process::id())),
    );
    std::fs::create_dir_all(&scratch.0).expect("a scratch directory");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "newest_stamp_later")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let sessions = DoltSessions::open(pool.clone());
    sessions
        .begin(run("nss5", at("2026-06-01T10:00:00Z")))
        .await
        .expect("begin ok");
    let only_the_day = jojobot_adapters::dolt::newest_stamp_from(&pool, date(2026, 6, 1))
        .await
        .expect("the store answers");
    sessions
        .begin(run("nss6", at("2026-06-02T01:00:00Z")))
        .await
        .expect("begin ok");

    let with_a_later_day = jojobot_adapters::dolt::newest_stamp_from(&pool, date(2026, 6, 1))
        .await
        .expect("the store answers");

    store.stop().await;
    assert_eq!(only_the_day, Some(at("2026-06-01T10:00:00Z")));
    assert_eq!(with_a_later_day, Some(at("2026-06-02T01:00:00Z")));
}

/// **A store stamped only before the day has no newest.** The control for the
/// cases above: the store holds stamps, and none is on or after this day.
#[tokio::test]
async fn a_store_stamped_only_before_the_day_has_no_newest_stamp() {
    let scratch = Scratch(
        std::env::temp_dir().join(format!("jojobot-newest-stamp-none-{}", std::process::id())),
    );
    std::fs::create_dir_all(&scratch.0).expect("a scratch directory");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "newest_stamp_none")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    DoltSessions::open(pool.clone())
        .begin(run("nss4", at("2026-05-31T23:00:00Z")))
        .await
        .expect("begin ok");

    let newest = jojobot_adapters::dolt::newest_stamp_from(&pool, date(2026, 6, 1))
        .await
        .expect("the store answers");

    store.stop().await;
    assert_eq!(newest, None);
}
